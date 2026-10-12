//! On-demand CPU profiles, when `AWZY_PROFILING=on`: `GET /debug/profile?seconds=30` samples every
//! thread for that long and answers with the functions that took the most samples, as plain text.
//! Off unless an operator turns it on: a profile names the code's functions and costs CPU while it
//! runs. Sampling is by signal (pprof-rs), so the container needs nothing installed.

use std::{collections::HashMap, time::Duration};

use axum::{extract::Query, http::StatusCode, routing::get, Router};
use serde::Deserialize;

/// Whether `AWZY_PROFILING` turns the profile route on.
pub fn enabled() -> bool {
    std::env::var("AWZY_PROFILING").is_ok_and(|v| v == "on")
}

pub fn routes() -> Router {
    Router::new().route("/debug/profile", get(profile))
}

#[derive(Deserialize)]
struct Params {
    seconds: Option<u64>,
}

async fn profile(Query(params): Query<Params>) -> (StatusCode, String) {
    let seconds = params.seconds.unwrap_or(30).clamp(1, 120);
    // The profiler is process-wide and blocks while it samples, so it runs on a blocking thread.
    let sampled = tokio::task::spawn_blocking(move || {
        let guard = pprof::ProfilerGuardBuilder::default()
            .frequency(499)
            .blocklist(&["libc", "libgcc", "pthread", "vdso"])
            .build()?;
        std::thread::sleep(Duration::from_secs(seconds));
        guard.report().build()
    })
    .await;
    match sampled {
        Ok(Ok(report)) => {
            let stacks: Vec<(Vec<String>, usize)> = report
                .data
                .iter()
                .map(|(frames, count)| {
                    let names = frames.frames.iter().flatten().map(|s| function_name(&s.name())).collect();
                    (names, usize::try_from(*count).unwrap_or(0))
                })
                .collect();
            (StatusCode::OK, top(&stacks, seconds, 40))
        }
        // Another profile is running, or the profiler couldn't start.
        Ok(Err(error)) => (StatusCode::CONFLICT, format!("couldn't profile: {error}\n")),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("couldn't profile: {error}\n")),
    }
}

/// A symbol's name without the hash rustc appends (`::h0123456789abcdef`).
fn function_name(symbol: &str) -> String {
    match symbol.rsplit_once("::h") {
        Some((name, hash)) if hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()) => name.to_string(),
        _ => symbol.to_string(),
    }
}

/// The `limit` functions with the most samples in their stacks (inclusive), and the most at the top
/// of them (self). Each stack's names run from the innermost frame out.
fn top(stacks: &[(Vec<String>, usize)], seconds: u64, limit: usize) -> String {
    let total: usize = stacks.iter().map(|(_, n)| n).sum();
    let mut inclusive: HashMap<&str, usize> = HashMap::new();
    let mut own: HashMap<&str, usize> = HashMap::new();
    for (names, n) in stacks {
        let mut seen = std::collections::HashSet::new();
        for name in names {
            if seen.insert(name.as_str()) {
                *inclusive.entry(name).or_default() += n;
            }
        }
        if let Some(innermost) = names.first() {
            *own.entry(innermost).or_default() += n;
        }
    }
    let table = |title: &str, counts: HashMap<&str, usize>| {
        let mut rows: Vec<_> = counts.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let mut out = format!("\n{title}\n");
        for (name, n) in rows.into_iter().take(limit) {
            let percent = if total == 0 { 0.0 } else { 100.0 * n as f64 / total as f64 };
            out.push_str(&format!("{percent:6.2}% {n:8}  {name}\n"));
        }
        out
    };
    format!(
        "CPU profile: {total} samples over {seconds} s{}{}",
        table("inclusive (in the stack)", inclusive),
        table("self (innermost frame)", own)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_are_ranked_by_samples_in_and_at_the_top_of_their_stacks() {
        let s = |names: &[&str], n| (names.iter().map(|n| n.to_string()).collect::<Vec<_>>(), n);
        let stacks = vec![
            s(&["cedar::evaluate", "pdp::check", "main"], 6),
            s(&["serde_json::parse", "pdp::check", "main"], 3),
            s(&["serde_json::parse", "serde_json::parse", "main"], 1),
        ];
        let report = top(&stacks, 30, 10);
        assert!(report.starts_with("CPU profile: 10 samples over 30 s"), "{report}");
        let (inclusive, own) = report.split_once("self (innermost frame)").unwrap();
        assert!(inclusive.contains("100.00%       10  main"), "{inclusive}");
        assert!(inclusive.contains(" 90.00%        9  pdp::check"), "{inclusive}");
        assert!(inclusive.contains(" 40.00%        4  serde_json::parse"), "counted once per stack: {inclusive}");
        assert!(own.contains(" 60.00%        6  cedar::evaluate"), "{own}");
    }

    #[test]
    fn rust_symbol_hashes_are_dropped() {
        assert_eq!(function_name("awzy_pdp::check::h0123456789abcdef"), "awzy_pdp::check");
        assert_eq!(function_name("std::thread::hold"), "std::thread::hold");
    }
}

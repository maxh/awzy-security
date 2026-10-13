//! The PDP's own metrics at `/metrics`, next to every service's baseline (ADR 0068): decisions by
//! verdict, failed checks, and check latency.
//!
//! awzy-reporter scrapes them for the install's heartbeat (decisions per second, p99, error rate;
//! ADR 0026), and a client's own monitoring can scrape them too. Counters are lock-free atomics
//! on the decision path.

use std::{
    sync::atomic::{AtomicU64, Ordering::Relaxed},
    time::Duration,
};

use std::{fmt::Write, sync::LazyLock};

use awzy_model::Verdict;
use awzy_server::metrics::{Counter, Histogram, LastSuccess};

/// Upper bounds of the latency buckets, in seconds. The PDP's target is p99 under 10 ms.
pub const BUCKETS: [f64; 12] = [0.0001, 0.00025, 0.0005, 0.001, 0.002, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 1.0];

#[derive(Default)]
pub struct Metrics {
    allow: AtomicU64,
    deny: AtomicU64,
    challenge: AtomicU64,
    /// Checks that failed: invalid requests, or refused before the first sync.
    errors: AtomicU64,
    /// Counts per bucket in BUCKETS, plus one for anything slower (+Inf).
    buckets: [AtomicU64; BUCKETS.len() + 1],
    /// Total check time, in nanoseconds.
    sum_nanos: AtomicU64,
}

impl Metrics {
    pub fn record(&self, verdict: Option<&Verdict>, elapsed: Duration) {
        match verdict {
            Some(Verdict::Allow) => &self.allow,
            Some(Verdict::Deny) => &self.deny,
            Some(Verdict::Challenge { .. }) => &self.challenge,
            None => &self.errors,
        }
        .fetch_add(1, Relaxed);
        let secs = elapsed.as_secs_f64();
        let bucket = BUCKETS.iter().position(|&b| secs <= b).unwrap_or(BUCKETS.len());
        self.buckets[bucket].fetch_add(1, Relaxed);
        self.sum_nanos.fetch_add(u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX), Relaxed);
    }

    /// The Prometheus text exposition format.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "# HELP awzy_pdp_decisions_total Decisions made, by verdict.\n# TYPE awzy_pdp_decisions_total counter\n",
        );
        for (verdict, n) in [("allow", &self.allow), ("deny", &self.deny), ("challenge", &self.challenge)] {
            out.push_str(&format!("awzy_pdp_decisions_total{{verdict=\"{verdict}\"}} {}\n", n.load(Relaxed)));
        }
        out.push_str("# HELP awzy_pdp_check_errors_total Checks that failed: invalid requests or refused before the first sync.\n# TYPE awzy_pdp_check_errors_total counter\n");
        out.push_str(&format!("awzy_pdp_check_errors_total {}\n", self.errors.load(Relaxed)));
        out.push_str("# HELP awzy_pdp_check_duration_seconds Time to decide one check.\n# TYPE awzy_pdp_check_duration_seconds histogram\n");
        let mut cumulative = 0;
        for (i, bound) in BUCKETS.iter().enumerate() {
            cumulative += self.buckets[i].load(Relaxed);
            out.push_str(&format!("awzy_pdp_check_duration_seconds_bucket{{le=\"{bound}\"}} {cumulative}\n"));
        }
        cumulative += self.buckets[BUCKETS.len()].load(Relaxed);
        out.push_str(&format!("awzy_pdp_check_duration_seconds_bucket{{le=\"+Inf\"}} {cumulative}\n"));
        out.push_str(&format!("awzy_pdp_check_duration_seconds_sum {}\n", self.sum_nanos.load(Relaxed) as f64 / 1e9));
        out.push_str(&format!("awzy_pdp_check_duration_seconds_count {cumulative}\n"));
        out
    }
}

impl awzy_server::metrics::Source for Metrics {
    fn render(&self, out: &mut String) {
        out.push_str(&Metrics::render(self));
        render_freshness(out);
    }
}

/// Time inside Cedar for one evaluation. A delegated check makes two, and a passed challenge one
/// more.
pub static CEDAR_EVALUATION: LazyLock<Histogram> = LazyLock::new(|| Histogram::new(&BUCKETS));

/// Resource lookups (ADR 0044): served fresh from the cache, fetched from awzy-context, or served
/// stale from the cache because awzy-context couldn't be reached.
pub static RESOURCE_CACHE_HITS: Counter = Counter::new();
pub static RESOURCE_FETCHES: Counter = Counter::new();
pub static RESOURCE_STALE_SERVED: Counter = Counter::new();

/// Checks that used the last containment list because neither the feed nor awzy-challenge
/// answered (ADR 0119).
pub static CONTAINMENTS_STALE_SERVED: Counter = Counter::new();

/// The freshness targets (platform plan): when the bundle and mode, and the principals snapshot,
/// were last synced. A sync that finds nothing new still counts: the PDP is as fresh as it can be.
pub static POLICY_SYNC: LazyLock<LastSuccess> = LazyLock::new(LastSuccess::default);
pub static PRINCIPALS_SYNC: LazyLock<LastSuccess> = LazyLock::new(LastSuccess::default);
/// awzy-context's resource change log, which drops changed resources from the cache (ADR 0077).
pub static RESOURCE_FEED: LazyLock<LastSuccess> = LazyLock::new(LastSuccess::default);

fn render_freshness(out: &mut String) {
    out.push_str("# HELP awzy_pdp_cedar_evaluation_seconds Time inside Cedar for one evaluation.\n");
    out.push_str("# TYPE awzy_pdp_cedar_evaluation_seconds histogram\n");
    CEDAR_EVALUATION.render(out, "awzy_pdp_cedar_evaluation_seconds", "");
    out.push_str("# HELP awzy_pdp_resource_lookups_total Resource lookups, by how they were served.\n");
    out.push_str("# TYPE awzy_pdp_resource_lookups_total counter\n");
    for (served, n) in
        [("cache", &RESOURCE_CACHE_HITS), ("fetched", &RESOURCE_FETCHES), ("stale", &RESOURCE_STALE_SERVED)]
    {
        let _ = writeln!(out, "awzy_pdp_resource_lookups_total{{served=\"{served}\"}} {}", n.get());
    }
    out.push_str("# HELP awzy_pdp_containments_stale_served_total Checks that used the last containment list because awzy-challenge couldn't be reached.\n");
    out.push_str("# TYPE awzy_pdp_containments_stale_served_total counter\n");
    let _ = writeln!(out, "awzy_pdp_containments_stale_served_total {}", CONTAINMENTS_STALE_SERVED.get());
    POLICY_SYNC.render(out, "awzy_pdp_policy_sync", "syncing the bundle and enforcement mode from awzy-policy");
    PRINCIPALS_SYNC.render(out, "awzy_pdp_principals_sync", "syncing principals from awzy-context");
    RESOURCE_FEED.render(out, "awzy_pdp_resource_feed", "awzy-context's resource change log");
}

#[cfg(test)]
mod tests {
    use awzy_model::Challenges;

    use super::*;

    #[test]
    fn counts_verdicts_and_buckets_latency() {
        let m = Metrics::default();
        m.record(Some(&Verdict::Allow), Duration::from_micros(300));
        m.record(Some(&Verdict::Deny), Duration::from_millis(3));
        m.record(Some(&Verdict::Challenge { challenges: Challenges::default() }), Duration::from_secs(2));
        m.record(None, Duration::from_micros(50));
        let text = m.render();
        for line in [
            r#"awzy_pdp_decisions_total{verdict="allow"} 1"#,
            r#"awzy_pdp_decisions_total{verdict="challenge"} 1"#,
            "awzy_pdp_check_errors_total 1",
            r#"awzy_pdp_check_duration_seconds_bucket{le="0.0001"} 1"#,
            r#"awzy_pdp_check_duration_seconds_bucket{le="0.0005"} 2"#,
            r#"awzy_pdp_check_duration_seconds_bucket{le="0.005"} 3"#,
            r#"awzy_pdp_check_duration_seconds_bucket{le="1"} 3"#,
            r#"awzy_pdp_check_duration_seconds_bucket{le="+Inf"} 4"#,
            "awzy_pdp_check_duration_seconds_count 4",
        ] {
            assert!(text.contains(line), "missing {line:?} in\n{text}");
        }
    }
}

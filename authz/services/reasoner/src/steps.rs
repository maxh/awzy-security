//! Each step of a run, on `/metrics` as `awzy_reasoner_steps_total`, by step (`scoring`,
//! `containments`, `investigations`) and outcome (`ok`, `failed`). A failed step only waits for
//! the next run, so without this a step that always fails looks like one with nothing to do: the
//! portal's page is just empty. An alert watches it.

use std::{
    collections::BTreeMap,
    fmt::Write,
    sync::{Arc, Mutex, Once},
};

static COUNTS: Mutex<BTreeMap<(&'static str, &'static str), u64>> = Mutex::new(BTreeMap::new());
static REGISTER: Once = Once::new();

/// One run of `step`, which succeeded or not.
pub fn count(step: &'static str, ok: bool) {
    REGISTER.call_once(|| awzy_server::metrics::register(Arc::new(Steps)));
    let outcome = if ok { "ok" } else { "failed" };
    *COUNTS.lock().unwrap_or_else(|e| e.into_inner()).entry((step, outcome)).or_default() += 1;
}

struct Steps;

impl awzy_server::metrics::Source for Steps {
    fn render(&self, out: &mut String) {
        out.push_str(
            "# HELP awzy_reasoner_steps_total Steps of awzy-reasoner's runs, by outcome.\n\
             # TYPE awzy_reasoner_steps_total counter\n",
        );
        for ((step, outcome), n) in COUNTS.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            let _ = writeln!(out, "awzy_reasoner_steps_total{{step=\"{step}\",outcome=\"{outcome}\"}} {n}");
        }
    }
}

#[cfg(test)]
mod tests {
    use awzy_server::metrics::Source;

    use super::*;

    #[test]
    fn each_step_is_counted_by_outcome() {
        count("only_in_this_test", false);
        count("only_in_this_test", false);
        count("only_in_this_test", true);
        let mut out = String::new();
        Steps.render(&mut out);
        assert!(out.contains("awzy_reasoner_steps_total{step=\"only_in_this_test\",outcome=\"failed\"} 2"), "{out}");
        assert!(out.contains("awzy_reasoner_steps_total{step=\"only_in_this_test\",outcome=\"ok\"} 1"), "{out}");
    }
}

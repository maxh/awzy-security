//! Calls a guard refused, on `/metrics` as `awzy_service_calls_refused_total`: by the method or
//! route (its pattern, never an ID in it), why, and for a caller its group doesn't allow, who.
//! Between awzy's own services this stays at zero. Anything else is a bug the tests can't see: a
//! caller missing from a group in the chart, or a method in no group (ADR 0055). An alert watches
//! it.

use std::{
    collections::BTreeMap,
    fmt::Write,
    sync::{Arc, Mutex, Once},
};

/// Why a call was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Reason {
    /// No rule names the method: no caller may call it.
    Unlisted,
    /// A valid caller its group doesn't allow.
    NotAllowed,
    /// No valid service token.
    Unauthenticated,
}

impl Reason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unlisted => "unlisted",
            Self::NotAllowed => "not_allowed",
            Self::Unauthenticated => "unauthenticated",
        }
    }
}

type Key = (String, Reason, String);

static COUNTS: Mutex<BTreeMap<Key, u64>> = Mutex::new(BTreeMap::new());
static REGISTER: Once = Once::new();

/// Adds the counter to this process's `/metrics`, once: from any guard being built.
pub(crate) fn register() {
    REGISTER.call_once(|| awzy_server::metrics::register(Arc::new(Refused)));
}

/// One refusal of a call to `route`, by `caller` (`namespace/service-account`) when known.
pub(crate) fn count(route: &str, reason: Reason, caller: Option<&str>) {
    let key = (route.to_string(), reason, caller.unwrap_or_default().to_string());
    *COUNTS.lock().unwrap_or_else(|e| e.into_inner()).entry(key).or_default() += 1;
}

struct Refused;

impl awzy_server::metrics::Source for Refused {
    fn render(&self, out: &mut String) {
        out.push_str(
            "# HELP awzy_service_calls_refused_total Calls a service-token guard refused.\n\
             # TYPE awzy_service_calls_refused_total counter\n",
        );
        for ((route, reason, caller), n) in COUNTS.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            let _ = writeln!(
                out,
                "awzy_service_calls_refused_total{{route=\"{}\",reason=\"{}\",caller=\"{}\"}} {n}",
                escape(route),
                reason.as_str(),
                escape(caller)
            );
        }
    }
}

fn escape(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use awzy_server::metrics::Source;

    #[test]
    fn refusals_are_counted_by_route_reason_and_caller() {
        count("/awzy.test.v1.T/Unlisted", Reason::Unlisted, None);
        count("/awzy.test.v1.T/Unlisted", Reason::Unlisted, None);
        count("/v1/things/{id}", Reason::NotAllowed, Some("awzy-portal/awzy"));
        let mut out = String::new();
        Refused.render(&mut out);
        assert!(out.contains("awzy_service_calls_refused_total{route=\"/awzy.test.v1.T/Unlisted\",reason=\"unlisted\",caller=\"\"} 2"), "{out}");
        assert!(
            out.contains("{route=\"/v1/things/{id}\",reason=\"not_allowed\",caller=\"awzy-portal/awzy\"} 1"),
            "{out}"
        );
    }
}

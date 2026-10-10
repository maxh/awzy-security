//! The gateway's own metrics at `/metrics` (ADR 0068): what happened to each checked call, and
//! especially what an outage of awzy-pdp did (ADR 0030).

use awzy_server::metrics::{Counter, Source};

/// Checked calls, by outcome. `monitor` forwarded a call policy would block.
pub static FORWARDED: Counter = Counter::new();
pub static BLOCKED: Counter = Counter::new();
pub static MONITOR_FORWARDED: Counter = Counter::new();

/// Calls made while awzy-pdp couldn't be reached: forwarded because the install was monitoring,
/// a recent allow replayed, or denied.
pub static OUTAGE_MONITOR_FORWARDED: Counter = Counter::new();
pub static OUTAGE_REPLAYED: Counter = Counter::new();
pub static OUTAGE_DENIED: Counter = Counter::new();

pub struct GatewayMetrics;

impl Source for GatewayMetrics {
    fn render(&self, out: &mut String) {
        out.push_str("# HELP awzy_gateway_checks_total Tool calls checked with awzy-pdp, by outcome.\n");
        out.push_str("# TYPE awzy_gateway_checks_total counter\n");
        for (outcome, n) in
            [("forwarded", &FORWARDED), ("blocked", &BLOCKED), ("monitor_forwarded", &MONITOR_FORWARDED)]
        {
            out.push_str(&format!("awzy_gateway_checks_total{{outcome=\"{outcome}\"}} {}\n", n.get()));
        }
        out.push_str("# HELP awzy_gateway_pdp_unreachable_total Tool calls made while awzy-pdp couldn't be reached, by what the gateway did.\n");
        out.push_str("# TYPE awzy_gateway_pdp_unreachable_total counter\n");
        for (outcome, n) in [
            ("monitor_forwarded", &OUTAGE_MONITOR_FORWARDED),
            ("replayed", &OUTAGE_REPLAYED),
            ("denied", &OUTAGE_DENIED),
        ] {
            out.push_str(&format!("awzy_gateway_pdp_unreachable_total{{outcome=\"{outcome}\"}} {}\n", n.get()));
        }
    }
}

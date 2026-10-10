//! The heartbeat's freshness numbers, from the `/metrics` of the PDPs, the gateways, and the
//! connectors (ADR 0068): the worst age of each sync across replicas, and the gateways' calls made
//! while the PDP was unreachable since the last heartbeat.

use std::collections::HashMap;

/// What one pod's `/metrics` says.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scrape {
    pub containment_feed_age: Option<f64>,
    pub policy_sync_age: Option<f64>,
    pub principals_sync_age: Option<f64>,
    /// The worst of the connectors' syncs.
    pub connector_sync_age: Option<f64>,
    /// `awzy_gateway_pdp_unreachable_total`, summed over its outcomes.
    pub gateway_unreachable_total: Option<u64>,
}

pub fn parse(text: &str) -> Scrape {
    let mut s = Scrape::default();
    let worst = |slot: &mut Option<f64>, v: f64| *slot = Some(slot.map_or(v, |w: f64| w.max(v)));
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let Some((name, value)) = line.rsplit_once(' ') else { continue };
        let Ok(value) = value.parse::<f64>() else { continue };
        match name {
            "awzy_pdp_containment_feed_age_seconds" => worst(&mut s.containment_feed_age, value),
            "awzy_pdp_policy_sync_age_seconds" => worst(&mut s.policy_sync_age, value),
            "awzy_pdp_principals_sync_age_seconds" => worst(&mut s.principals_sync_age, value),
            n if n.starts_with("awzy_connectors_") && n.ends_with("_sync_age_seconds") => {
                worst(&mut s.connector_sync_age, value)
            }
            n if n.starts_with("awzy_gateway_pdp_unreachable_total{") => {
                *s.gateway_unreachable_total.get_or_insert(0) += value as u64;
            }
            _ => {}
        }
    }
    s
}

/// The heartbeat's freshness figures.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Freshness {
    pub containment_feed_age: Option<f64>,
    pub policy_sync_age: Option<f64>,
    pub principals_sync_age: Option<f64>,
    pub connector_sync_age: Option<f64>,
    /// None until some gateway has been scraped twice.
    pub gateway_unreachable: Option<u64>,
}

/// Remembers each gateway's last counter, keyed by pod (or URL), to count calls between heartbeats.
#[derive(Default)]
pub struct Tracker {
    last: HashMap<String, u64>,
}

impl Tracker {
    pub fn update(&mut self, scrapes: &[(String, Scrape)]) -> Freshness {
        let mut f = Freshness::default();
        let worst = |slot: &mut Option<f64>, v: Option<f64>| {
            if let Some(v) = v {
                *slot = Some(slot.map_or(v, |w: f64| w.max(v)));
            }
        };
        let mut counters = HashMap::new();
        for (key, s) in scrapes {
            worst(&mut f.containment_feed_age, s.containment_feed_age);
            worst(&mut f.policy_sync_age, s.policy_sync_age);
            worst(&mut f.principals_sync_age, s.principals_sync_age);
            worst(&mut f.connector_sync_age, s.connector_sync_age);
            let Some(now) = s.gateway_unreachable_total else { continue };
            if let Some(&prev) = self.last.get(key) {
                // A restarted gateway counts from zero.
                *f.gateway_unreachable.get_or_insert(0) += if now >= prev { now - prev } else { now };
            }
            counters.insert(key.clone(), now);
        }
        self.last = counters;
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_worst_ages_and_sums_the_gateways_outcomes() {
        let s = parse(
            "# HELP x\nawzy_pdp_containment_feed_age_seconds 0.400\nawzy_pdp_policy_sync_age_seconds 4.2\n\
             awzy_connectors_drive_sync_age_seconds 30\nawzy_connectors_jira_sync_age_seconds 900\n\
             awzy_gateway_pdp_unreachable_total{outcome=\"replayed\"} 3\nawzy_gateway_pdp_unreachable_total{outcome=\"denied\"} 2\n",
        );
        assert_eq!(s.containment_feed_age, Some(0.4));
        assert_eq!(s.connector_sync_age, Some(900.0), "the stalest connector");
        assert_eq!(s.gateway_unreachable_total, Some(5));
        assert_eq!(s.principals_sync_age, None, "no awzy-context");
    }

    #[test]
    fn counts_outage_calls_between_heartbeats_across_replicas_and_restarts() {
        let gw = |n| Scrape { gateway_unreachable_total: Some(n), ..Scrape::default() };
        let pdp = |age| Scrape { policy_sync_age: Some(age), ..Scrape::default() };
        let mut t = Tracker::default();
        let first =
            t.update(&[("a".into(), gw(5)), ("b".into(), gw(1)), ("p1".into(), pdp(3.0)), ("p2".into(), pdp(9.0))]);
        assert_eq!(first.gateway_unreachable, None, "nothing to compare with yet");
        assert_eq!(first.policy_sync_age, Some(9.0), "the worst PDP");
        let second = t.update(&[("a".into(), gw(8)), ("b".into(), gw(0))]);
        assert_eq!(second.gateway_unreachable, Some(3), "a's three; b restarted and has made none");
    }
}

//! The heartbeat's metrics from the PDPs' Prometheus `/metrics` (awzy-pdp's metrics module):
//! decisions per second, error rate, and p99 latency over the interval since the last scrape,
//! merged across PDP replicas.

use std::collections::HashMap;

/// One PDP's counters at one moment.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub decisions: u64,
    pub errors: u64,
    /// Cumulative histogram buckets: (upper bound in seconds, count), the last one +Inf.
    pub buckets: Vec<(f64, u64)>,
}

/// Reads the counters out of the Prometheus text format.
pub fn parse(text: &str) -> Sample {
    let mut s = Sample::default();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let Some((name, value)) = line.rsplit_once(' ') else { continue };
        let Ok(value) = value.parse::<f64>() else { continue };
        if name.starts_with("awzy_pdp_decisions_total{") {
            s.decisions += value as u64;
        } else if name == "awzy_pdp_check_errors_total" {
            s.errors += value as u64;
        } else if let Some(rest) = name.strip_prefix("awzy_pdp_check_duration_seconds_bucket{le=\"") {
            let le = rest.trim_end_matches("\"}");
            let bound = if le == "+Inf" { f64::INFINITY } else { le.parse().unwrap_or(f64::INFINITY) };
            s.buckets.push((bound, value as u64));
        }
    }
    s
}

/// What happened between two samples of one PDP. A restarted PDP (counters went down) counts
/// from zero.
fn delta(prev: Option<&Sample>, now: &Sample) -> Sample {
    let Some(prev) = prev.filter(|p| p.decisions <= now.decisions && p.errors <= now.errors) else {
        return now.clone();
    };
    let buckets = now
        .buckets
        .iter()
        .map(|&(le, n)| (le, n.saturating_sub(prev.buckets.iter().find(|b| b.0 == le).map_or(0, |b| b.1))))
        .collect();
    Sample { decisions: now.decisions - prev.decisions, errors: now.errors - prev.errors, buckets }
}

/// The 99th percentile in milliseconds, interpolated within its bucket.
fn p99_ms(buckets: &[(f64, u64)]) -> Option<f64> {
    let total = buckets.last()?.1;
    if total == 0 {
        return None;
    }
    let target = 0.99 * total as f64;
    let mut lower = (0.0, 0u64);
    for &(le, n) in buckets {
        if n as f64 >= target {
            if le.is_infinite() {
                return Some(lower.0 * 1000.0);
            }
            let within = (target - lower.1 as f64) / ((n - lower.1).max(1)) as f64;
            return Some((lower.0 + (le - lower.0) * within) * 1000.0);
        }
        lower = (le, n);
    }
    None
}

/// Remembers each PDP's last sample, keyed by pod (or URL), to turn counters into rates.
#[derive(Default)]
pub struct Tracker {
    last: HashMap<String, Sample>,
    last_at: Option<std::time::Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    pub decisions_per_second: f64,
    pub error_rate: Option<f64>,
    pub p99_ms: Option<f64>,
}

impl Tracker {
    /// Rates since the previous call, or None on the first call (nothing to compare with).
    pub fn update(&mut self, samples: HashMap<String, Sample>, now: std::time::Instant) -> Option<Rates> {
        let elapsed = self.last_at.map(|t| now.duration_since(t).as_secs_f64());
        let mut decisions = 0;
        let mut errors = 0;
        let mut merged: Vec<(f64, u64)> = Vec::new();
        for (key, sample) in &samples {
            let d = delta(self.last.get(key), sample);
            decisions += d.decisions;
            errors += d.errors;
            for (le, n) in d.buckets {
                match merged.iter_mut().find(|b| b.0 == le) {
                    Some(b) => b.1 += n,
                    None => merged.push((le, n)),
                }
            }
        }
        merged.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.last = samples;
        self.last_at = Some(now);
        let elapsed = elapsed.filter(|e| *e > 0.0)?;
        let total = decisions + errors;
        Some(Rates {
            decisions_per_second: decisions as f64 / elapsed,
            error_rate: (total > 0).then(|| errors as f64 / total as f64),
            p99_ms: p99_ms(&merged),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn text(decisions: u64, errors: u64, buckets: &[(&str, u64)]) -> String {
        let mut t = format!("# TYPE x counter\nawzy_pdp_decisions_total{{verdict=\"allow\"}} {decisions}\nawzy_pdp_decisions_total{{verdict=\"deny\"}} 0\nawzy_pdp_check_errors_total {errors}\n");
        for (le, n) in buckets {
            t.push_str(&format!("awzy_pdp_check_duration_seconds_bucket{{le=\"{le}\"}} {n}\n"));
        }
        t
    }

    #[test]
    fn rates_across_replicas_and_restarts() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        let first = HashMap::from([
            ("a".into(), parse(&text(100, 0, &[("0.001", 100), ("0.01", 100), ("+Inf", 100)]))),
            ("b".into(), parse(&text(50, 0, &[("0.001", 50), ("0.01", 50), ("+Inf", 50)]))),
        ]);
        assert_eq!(tracker.update(first, t0), None, "nothing to compare the first scrape with");

        // In 10 s: a made 1000 more (990 under 1 ms, 10 under 10 ms); b restarted, so its counters
        // went down, and has made 40 since, plus 10 errors.
        let second = HashMap::from([
            ("a".into(), parse(&text(1100, 0, &[("0.001", 1090), ("0.01", 1100), ("+Inf", 1100)]))),
            ("b".into(), parse(&text(40, 10, &[("0.001", 50), ("0.01", 50), ("+Inf", 50)]))),
        ]);
        let r = tracker.update(second, t0 + Duration::from_secs(10)).unwrap();
        assert_eq!(r.decisions_per_second, 104.0, "(1000 + 40) / 10 s");
        assert_eq!(r.error_rate, Some(10.0 / 1050.0));
        let p99 = r.p99_ms.unwrap();
        assert!((0.9..=1.0).contains(&p99), "1040 of 1050 checks finished within 1 ms: {p99}");
    }

    #[test]
    fn p99_interpolates_and_handles_the_overflow_bucket() {
        let p99 = p99_ms(&[(0.001, 0), (0.01, 100), (f64::INFINITY, 100)]).unwrap();
        assert!((p99 - 9.91).abs() < 1e-9, "99% of the way from 1 ms to 10 ms: {p99}");
        assert_eq!(p99_ms(&[(0.001, 0), (1.0, 0), (f64::INFINITY, 100)]), Some(1000.0), "slower than the last bound");
        assert_eq!(p99_ms(&[(0.001, 0), (f64::INFINITY, 0)]), None);
    }
}

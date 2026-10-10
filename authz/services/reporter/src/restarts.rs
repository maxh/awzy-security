//! Restarts over the last 24 hours. Kubernetes reports only each pod's restarts since it
//! started, so the reporter keeps a day of samples per pod and reports the difference.

use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Duration, Utc};

const WINDOW: Duration = Duration::hours(24);

#[derive(Default)]
pub struct RestartTracker {
    samples: HashMap<String, VecDeque<(DateTime<Utc>, u32)>>,
}

impl RestartTracker {
    /// Records pods' restart counts at `now` and returns each pod's restarts in the last 24
    /// hours. A pod that started within the window counts all its restarts. For an older pod the
    /// count covers only as long as the reporter has been watching it.
    pub fn update(
        &mut self,
        now: DateTime<Utc>,
        pods: &[(String, u32, Option<DateTime<Utc>>)],
    ) -> HashMap<String, u32> {
        self.samples.retain(|uid, _| pods.iter().any(|p| &p.0 == uid));
        let mut out = HashMap::new();
        for (uid, restarts, started_at) in pods {
            let samples = self.samples.entry(uid.clone()).or_default();
            samples.push_back((now, *restarts));
            while samples.front().is_some_and(|(t, _)| now - *t > WINDOW) {
                samples.pop_front();
            }
            let recent = started_at.is_some_and(|s| now - s <= WINDOW);
            let baseline = if recent { 0 } else { samples.front().map_or(*restarts, |s| s.1) };
            out.insert(uid.clone(), restarts.saturating_sub(baseline));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_only_the_last_day() {
        let mut t = RestartTracker::default();
        let start = Utc::now() - Duration::days(10);
        let now = Utc::now();
        // An old pod with 7 restarts, seen for the first time: none in the window yet.
        assert_eq!(t.update(now, &[("old".into(), 7, Some(start))])["old"], 0);
        // Two more over the next hours.
        assert_eq!(t.update(now + Duration::hours(3), &[("old".into(), 9, Some(start))])["old"], 2);
        // A day and a bit later, the first sample has left the window.
        assert_eq!(t.update(now + Duration::hours(25), &[("old".into(), 10, Some(start))])["old"], 1);
        // A pod that started an hour ago counts everything.
        let new = t.update(now, &[("new".into(), 3, Some(now - Duration::hours(1)))]);
        assert_eq!(new["new"], 3);
        assert!(!t.samples.contains_key("old"), "gone pods are forgotten");
    }
}

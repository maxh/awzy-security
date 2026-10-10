//! Deterministic risk scores (ADR 0056): a principal's last day against its own recent days and
//! against its team's last day. No LLM, so the same activity always scores the same, and a
//! later LLM investigation can raise a score but has this one as its floor.

/// What one principal did in one day.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Day {
    pub accesses: u64,
    pub resources: u64,
    pub denied: u64,
}

/// A score from 0 to 100 and what raised it.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub score: i32,
    pub reasons: Vec<String>,
}

/// Points per multiple above normal, and the most one comparison can add.
const POINTS_PER_MULTIPLE: f64 = 15.0;
const MOST_PER_COMPARISON: f64 = 50.0;
/// Points per denied request in the last day, and the most denials can add.
const POINTS_PER_DENIAL: u64 = 5;
const MOST_FOR_DENIALS: u64 = 20;
/// Active days of history before a principal's own baseline counts.
const MIN_BASELINE_DAYS: usize = 5;
/// Other active members a team needs to be a peer group.
const MIN_PEERS: usize = 3;
/// Normal is never taken as fewer resources than this a day, so a quiet day that doubles is
/// still quiet.
const FLOOR_RESOURCES: f64 = 5.0;

/// `last_day` scored against `baseline` (the principal's earlier active days) and `peers` (the
/// last day of the other active members of its team). Either comparison is skipped when there's
/// too little to compare with.
pub fn score(last_day: Day, baseline: &[Day], peers: &[Day]) -> Score {
    let mut points = 0.0;
    let mut reasons = Vec::new();

    if baseline.len() >= MIN_BASELINE_DAYS {
        let usual = baseline.iter().map(|d| d.resources as f64).sum::<f64>() / baseline.len() as f64;
        let p = above(last_day.resources, usual);
        if p >= 1.0 {
            points += p;
            reasons.push(format!("opened {} resources in the last day; usually {usual:.0} a day", last_day.resources));
        }
    }

    if peers.len() >= MIN_PEERS {
        let mut theirs: Vec<u64> = peers.iter().map(|d| d.resources).collect();
        theirs.sort_unstable();
        let median = theirs[theirs.len() / 2] as f64;
        let p = above(last_day.resources, median);
        if p >= 1.0 {
            points += p;
            reasons.push(format!(
                "opened {} resources in the last day; its team's median is {median:.0}",
                last_day.resources
            ));
        }
    }

    if last_day.denied > 0 {
        points += (last_day.denied * POINTS_PER_DENIAL).min(MOST_FOR_DENIALS) as f64;
        reasons.push(format!("{} requests denied in the last day", last_day.denied));
    }

    Score { score: points.round().clamp(0.0, 100.0) as i32, reasons }
}

/// Points for `observed` resources against a normal of `usual`.
fn above(observed: u64, usual: f64) -> f64 {
    let multiple = observed as f64 / usual.max(FLOOR_RESOURCES);
    ((multiple - 1.0) * POINTS_PER_MULTIPLE).clamp(0.0, MOST_PER_COMPARISON)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(resources: u64) -> Day {
        Day { accesses: resources * 2, resources, denied: 0 }
    }

    #[test]
    fn an_ordinary_day_scores_near_zero() {
        let s = score(day(11), &[day(10); 20], &[day(9), day(10), day(12)]);
        assert!(s.score < 10, "{s:?}");
    }

    #[test]
    fn opening_six_times_the_usual_and_the_peers_is_high_risk() {
        let s = score(day(60), &[day(10); 20], &[day(9), day(10), day(12)]);
        assert_eq!(s.score, 100);
        assert_eq!(s.reasons.len(), 2, "{s:?}");
    }

    #[test]
    fn one_comparison_alone_is_capped_below_the_challenge_threshold() {
        // A new hire with no history, far above peers: suspicious, not challenged on that alone.
        let s = score(day(60), &[], &[day(9), day(10), day(12)]);
        assert_eq!(s.score, 50);
    }

    #[test]
    fn too_little_history_or_too_few_peers_compares_with_nothing() {
        assert_eq!(score(day(60), &[day(10); 4], &[day(10); 2]).score, 0);
    }

    #[test]
    fn a_quiet_day_that_doubles_is_still_quiet() {
        assert_eq!(score(day(4), &[day(2); 20], &[day(1); 5]).score, 0);
    }

    #[test]
    fn denials_add_up_to_a_limit() {
        let s = score(Day { denied: 2, ..day(10) }, &[day(10); 20], &[]);
        assert_eq!(s.score, 10);
        assert_eq!(score(Day { denied: 50, ..day(10) }, &[day(10); 20], &[]).score, 20);
    }
}

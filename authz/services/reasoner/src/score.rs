//! Deterministic risk scores (ADR 0056): a principal's last day against its own recent days and
//! against its team's last day, and what it opened that neither it nor its team had (ADR 0064).
//! No LLM, so the same activity always scores the same, and a
//! later LLM investigation can raise a score but has this one as its floor.

use crate::activity::HISTORY_DAYS;

/// What one principal did in one day.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Day {
    pub accesses: u64,
    pub resources: u64,
    pub denied: u64,
    /// Resources opened that neither the principal nor its team had opened before (ADR 0064).
    /// Only counted for the last day.
    pub unfamiliar: u64,
    /// Rows touched by accesses that said their kind (#288), each weighted by
    /// [`ACCESS_WEIGHTS`]: a list's rows count for less than a read's, an export's for more.
    pub rows: f64,
    /// Of those, the rows exported, unweighted, to name in a reason.
    pub exported: u64,
}

/// How much each kind of access counts per row it touched, against a read or a write's 1.
/// Seeing rows in a list moves little data; exporting them moves all of it out.
pub struct AccessWeights {
    pub listed: f64,
    pub exported: f64,
}

pub const ACCESS_WEIGHTS: AccessWeights = AccessWeights { listed: 0.25, exported: 4.0 };

impl Day {
    /// How much it touched: the resources it opened, plus its weighted rows.
    pub fn volume(&self) -> f64 {
        self.resources as f64 + self.rows
    }

    /// The volume in words, for a reason.
    fn touched(&self) -> String {
        let mut words = format!("opened {} resources", self.resources);
        if self.rows > 0.0 {
            words.push_str(&format!(" and touched rows worth {:.0}", self.rows));
            if self.exported > 0 {
                words.push_str(&format!(", {} of them exported", self.exported));
            }
        }
        words
    }
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
/// Unfamiliar resources a day may hold before they count (the odd document from another team),
/// the points for each one past that, and the most they can add: on their own, enough to make
/// a principal's risk elevated, never enough to challenge everything it does.
const USUAL_UNFAMILIAR: u64 = 2;
const POINTS_PER_UNFAMILIAR: u64 = 10;
const MOST_FOR_UNFAMILIAR: u64 = 50;
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
        let usual = baseline.iter().map(Day::volume).sum::<f64>() / baseline.len() as f64;
        let p = above(last_day.volume(), usual);
        if p >= 1.0 {
            points += p;
            reasons.push(format!("{} in the last day; usually {usual:.0} a day", last_day.touched()));
        }
    }

    if peers.len() >= MIN_PEERS {
        let mut theirs: Vec<f64> = peers.iter().map(Day::volume).collect();
        theirs.sort_unstable_by(f64::total_cmp);
        let median = theirs[theirs.len() / 2];
        let p = above(last_day.volume(), median);
        if p >= 1.0 {
            points += p;
            reasons.push(format!("{} in the last day; its team's median is {median:.0}", last_day.touched()));
        }
    }

    if last_day.unfamiliar > USUAL_UNFAMILIAR {
        points += ((last_day.unfamiliar - USUAL_UNFAMILIAR) * POINTS_PER_UNFAMILIAR).min(MOST_FOR_UNFAMILIAR) as f64;
        reasons.push(format!(
            "opened {} resources that neither it nor its team had opened in {HISTORY_DAYS} days",
            last_day.unfamiliar
        ));
    }

    if last_day.denied > 0 {
        points += (last_day.denied * POINTS_PER_DENIAL).min(MOST_FOR_DENIALS) as f64;
        reasons.push(format!("{} requests denied in the last day", last_day.denied));
    }

    Score { score: points.round().clamp(0.0, 100.0) as i32, reasons }
}

/// Points for an `observed` volume against a normal of `usual`.
fn above(observed: f64, usual: f64) -> f64 {
    let multiple = observed / usual.max(FLOOR_RESOURCES);
    ((multiple - 1.0) * POINTS_PER_MULTIPLE).clamp(0.0, MOST_PER_COMPARISON)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(resources: u64) -> Day {
        Day { accesses: resources * 2, resources, ..Day::default() }
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
    fn unfamiliar_territory_raises_risk_to_elevated_and_no_further() {
        // The suddenly foolish administrator's basic questions: an ordinary amount of reading, but
        // seven documents nobody on the team has needed in a month.
        let s = score(Day { unfamiliar: 7, ..day(12) }, &[day(12); 20], &[day(11), day(12), day(13)]);
        assert_eq!(s.score, 50, "{s:?}");
        assert!(s.reasons[0].contains("neither it nor its team"), "{s:?}");
        assert_eq!(score(Day { unfamiliar: 40, ..day(12) }, &[day(12); 20], &[]).score, 50);
    }

    #[test]
    fn the_odd_document_from_another_team_is_ordinary() {
        assert_eq!(score(Day { unfamiliar: 2, ..day(12) }, &[day(12); 20], &[]).score, 0);
    }

    #[test]
    fn denials_add_up_to_a_limit() {
        let s = score(Day { denied: 2, ..day(10) }, &[day(10); 20], &[]);
        assert_eq!(s.score, 10);
        assert_eq!(score(Day { denied: 50, ..day(10) }, &[day(10); 20], &[]).score, 20);
    }

    /// The paper's "500 percent more files than their peer group" counts rows, not events, and
    /// an export of them counts for more than the same rows listed (#288).
    #[test]
    fn a_bulk_export_moves_risk_more_than_listing_the_same_rows() {
        let baseline = [day(12); 20];
        let peers = [day(11), day(12), day(13)];
        let with_rows = |weight: f64, exported: u64| Day { rows: 40.0 * weight, exported, ..day(12) };
        let listed = score(with_rows(ACCESS_WEIGHTS.listed, 0), &baseline, &peers);
        let exported = score(with_rows(ACCESS_WEIGHTS.exported, 40), &baseline, &peers);
        assert!(exported.score > listed.score, "{exported:?} against {listed:?}");
        assert!(exported.reasons[0].contains("40 of them exported"), "{:?}", exported.reasons);
        // Listed, 40 rows weigh 10 resources: 22 against 12 a day still stands out, a little.
        assert!(listed.score > 0, "a 40-row list still stands out against 12 a day");
    }
}

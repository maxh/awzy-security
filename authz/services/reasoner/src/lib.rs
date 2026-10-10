//! awzy-reasoner: the slow path. Scores each person's and service account's risk from their
//! activity in ClickHouse and writes the scores to awzy-context, where the PDPs pick them up
//! (ADR 0056). Investigations come later.

pub mod activity;
pub mod context;
pub mod score;

use std::{collections::HashMap, time::Duration};

use axum::Router;
use chrono::{DateTime, Utc};

use crate::{
    activity::{History, Principal},
    context::ContextClient,
    score::{Day, Score},
};

/// Routes this service serves in addition to the shared health routes.
pub fn router() -> Router {
    Router::new()
}

/// One principal's score.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub principal: Principal,
    pub score: Score,
}

/// awzy-context takes IDs up to this long.
const MAX_ID_LEN: usize = 256;

/// Scores every principal with activity in `history`. Its peers are the other members of its
/// team (from `teams`) who were active in the last day.
pub fn score_all(history: &History, teams: &HashMap<Principal, String>) -> Vec<Scored> {
    let mut last_days: HashMap<&str, Vec<(&Principal, Day)>> = HashMap::new();
    for (p, days) in history {
        if let (Some(team), Some(day)) = (teams.get(p), days.get(&0)) {
            last_days.entry(team.as_str()).or_default().push((p, *day));
        }
    }
    history
        .iter()
        .filter(|(p, _)| !p.1.is_empty() && p.1.len() <= MAX_ID_LEN)
        .map(|(p, days)| {
            let last_day = days.get(&0).copied().unwrap_or_default();
            let baseline: Vec<Day> = days.iter().filter(|(ago, _)| **ago > 0).map(|(_, d)| *d).collect();
            let peers: Vec<Day> = teams
                .get(p)
                .and_then(|t| last_days.get(t.as_str()))
                .into_iter()
                .flatten()
                .filter(|(other, _)| *other != p)
                .map(|(_, d)| *d)
                .collect();
            Scored { principal: p.clone(), score: score::score(last_day, &baseline, &peers) }
        })
        .collect()
}

pub struct Reasoner {
    pub clickhouse: clickhouse::Client,
    pub context: ContextClient,
}

impl Reasoner {
    /// Scores everyone active in the history before `as_of` and writes the scores. Returns how
    /// many it wrote.
    pub async fn run_once(&self, as_of: DateTime<Utc>) -> anyhow::Result<usize> {
        let history = activity::load(&self.clickhouse, as_of).await?;
        let teams = self.context.teams().await?;
        let scores = score_all(&history, &teams);
        self.context.put(&scores).await?;
        Ok(scores.len())
    }

    /// Scores everyone every `interval`, for as long as the service runs. A failed run is
    /// logged and retried at the next interval; the PDPs keep the scores they have.
    pub async fn run(self, interval: Duration) {
        let mut ticks = tokio::time::interval(interval);
        loop {
            ticks.tick().await;
            match self.run_once(Utc::now()).await {
                Ok(n) => tracing::info!(scored = n, "risk scores written"),
                Err(e) => tracing::warn!(error = %e, "scoring failed; trying again next interval"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn human(id: &str) -> Principal {
        ("Human".into(), id.into())
    }

    /// Twenty active days of ten resources each, with `last_day` resources today.
    fn month(last_day: u64) -> BTreeMap<u32, Day> {
        (0..20)
            .map(|ago| (ago, Day { accesses: 10, resources: if ago == 0 { last_day } else { 10 }, denied: 0 }))
            .collect()
    }

    #[test]
    fn the_peer_outlier_scores_high_and_the_team_stays_low() {
        let history: History =
            [(human("ana"), month(60)), (human("ben"), month(10)), (human("cy"), month(11)), (human("dee"), month(9))]
                .into();
        let teams = ["ana", "ben", "cy", "dee"].map(|h| (human(h), "analysts".to_string())).into();
        let scores: HashMap<_, _> =
            score_all(&history, &teams).into_iter().map(|s| (s.principal.1.clone(), s.score.score)).collect();
        assert!(scores["ana"] > 80, "{scores:?}");
        for quiet in ["ben", "cy", "dee"] {
            assert!(scores[quiet] < 10, "{scores:?}");
        }
    }

    #[test]
    fn someone_quiet_today_is_scored_back_down() {
        let mut days = month(0);
        days.remove(&0);
        let scored = score_all(&[(human("ana"), days)].into(), &HashMap::new());
        assert_eq!(scored[0].score.score, 0);
    }
}

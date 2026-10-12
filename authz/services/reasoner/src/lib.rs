//! awzy-reasoner: the slow path. Scores each person's and service account's risk from their
//! activity in ClickHouse and writes the scores to awzy-context, where the PDPs pick them up
//! (ADR 0056). Contains people when the day's evidence adds up, in awzy-challenge (ADR 0066).
//! Investigates the principals it flags, within a monthly token budget, with a model whose
//! findings are checked in code ([`investigate`], ADR 0067; [`runs`], ADR 0071).

pub mod activity;
pub mod automation;
pub mod contain;
pub mod context;
pub mod investigate;
pub mod runs;
pub mod score;

use std::{collections::HashMap, time::Duration};

use axum::Router;
use chrono::{DateTime, Utc};

use crate::{
    activity::{History, Opened, Principal, MAX_OPENERS},
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
    /// For a person: how much their requests look like software using their credentials (ADR
    /// 0085). None for a service account, or too little to tell.
    pub automation: Option<Score>,
}

/// awzy-context takes IDs up to this long.
const MAX_ID_LEN: usize = 256;

/// How many resources each principal opened in the last day that neither it nor anyone on its
/// team had opened in the month before, or alongside it that day (ADR 0064). A principal with
/// no team is compared only with itself; a resource with more than [`MAX_OPENERS`] openers is
/// familiar to everyone.
pub fn unfamiliar(opened: &Opened, teams: &HashMap<Principal, String>) -> HashMap<Principal, u64> {
    let mut counts = HashMap::new();
    for openers in opened.values() {
        if openers.before.len() > MAX_OPENERS || openers.last_day.len() > MAX_OPENERS {
            continue;
        }
        for p in &openers.last_day {
            let team = teams.get(p);
            let teammate = |q: &Principal| q != p && team.is_some() && teams.get(q) == team;
            let familiar = openers.before.contains(p)
                || openers.before.iter().any(teammate)
                || openers.last_day.iter().any(teammate);
            if !familiar {
                *counts.entry(p.clone()).or_default() += 1;
            }
        }
    }
    counts
}

/// Scores every principal with activity in `history`. Its peers are the other members of its
/// team (from `teams`) who were active in the last day; what it opened is compared with what
/// they had opened (`opened`).
pub fn score_all(history: &History, opened: &Opened, teams: &HashMap<Principal, String>) -> Vec<Scored> {
    let unfamiliar = unfamiliar(opened, teams);
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
            let mut last_day = days.get(&0).copied().unwrap_or_default();
            last_day.unfamiliar = unfamiliar.get(p).copied().unwrap_or_default();
            let baseline: Vec<Day> = days.iter().filter(|(ago, _)| **ago > 0).map(|(_, d)| *d).collect();
            let peers: Vec<Day> = teams
                .get(p)
                .and_then(|t| last_days.get(t.as_str()))
                .into_iter()
                .flatten()
                .filter(|(other, _)| *other != p)
                .map(|(_, d)| *d)
                .collect();
            Scored { principal: p.clone(), score: score::score(last_day, &baseline, &peers), automation: None }
        })
        .collect()
}

pub struct Reasoner {
    pub clickhouse: clickhouse::Client,
    pub context: ContextClient,
    /// None when awzy-challenge or awzy-policy isn't configured: nobody is contained.
    pub containments: Option<contain::Containments>,
    /// None until the client turns investigations on (ADR 0071).
    pub investigations: Option<runs::Investigations<awzy_llm::ModelRouter>>,
}

impl Reasoner {
    /// Scores everyone active in the history before `as_of` and writes the scores. Returns how
    /// many it wrote.
    pub async fn run_once(&self, as_of: DateTime<Utc>) -> anyhow::Result<usize> {
        let history = activity::load(&self.clickhouse, as_of).await?;
        let opened = activity::load_opened(&self.clickhouse, as_of).await?;
        let teams = self.context.teams().await?;
        let mut scores = score_all(&history, &opened, &teams);
        let automation = automation::load(&self.clickhouse, as_of).await?;
        for s in &mut scores {
            s.automation = automation.get(&s.principal).copied().and_then(automation::score);
        }
        self.context.put(&scores).await?;
        // Scores are written whatever happens below; a failed step waits for the next run.
        let mut rejected = std::collections::BTreeMap::new();
        if let Some(c) = &self.containments {
            match self.contain(c, &scores, as_of).await {
                Ok(evidence) => rejected = evidence.rejected,
                Err(e) => tracing::warn!(error = %e, "automated containments skipped this run"),
            }
        }
        if let Some(inv) = &self.investigations {
            let containments = self.containments.as_ref();
            match inv.run(&self.clickhouse, &self.context, containments, &scores, &rejected, as_of).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(investigations = n, "investigations ran"),
                Err(e) => tracing::warn!(error = %e, "investigations stopped this run"),
            }
        }
        Ok(scores.len())
    }

    /// Contains whoever this run's scores and the day's evidence call for (ADR 0066), or records
    /// that it would have, by the `automated-containments` scope's mode.
    /// Returns the day's evidence, for the investigations that follow.
    async fn contain(
        &self,
        c: &contain::Containments,
        scores: &[Scored],
        as_of: DateTime<Utc>,
    ) -> anyhow::Result<contain::Evidence> {
        // Read first: if the mode can't be read, nobody is contained.
        let mode = c.mode().await?;
        let mut evidence = c.evidence(as_of).await?;
        evidence.exfiltration = contain::load_exfiltration(&self.clickhouse, as_of).await?;
        for p in contain::proposals(scores, &evidence) {
            c.contain(&p, mode).await?;
            tracing::warn!(human = %p.human, mode = mode.as_str(), reason = %p.reason, "automated containment");
        }
        Ok(evidence)
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
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::activity::Openers;

    fn human(id: &str) -> Principal {
        ("Human".into(), id.into())
    }

    /// Twenty active days of ten resources each, with `last_day` resources today.
    fn month(last_day: u64) -> BTreeMap<u32, Day> {
        (0..20)
            .map(|ago| (ago, Day { accesses: 10, resources: if ago == 0 { last_day } else { 10 }, ..Day::default() }))
            .collect()
    }

    #[test]
    fn the_peer_outlier_scores_high_and_the_team_stays_low() {
        let history: History =
            [(human("ana"), month(60)), (human("ben"), month(10)), (human("cy"), month(11)), (human("dee"), month(9))]
                .into();
        let teams = ["ana", "ben", "cy", "dee"].map(|h| (human(h), "analysts".to_string())).into();
        let scores: HashMap<_, _> = score_all(&history, &Opened::new(), &teams)
            .into_iter()
            .map(|s| (s.principal.1.clone(), s.score.score))
            .collect();
        assert!(scores["ana"] > 80, "{scores:?}");
        for quiet in ["ben", "cy", "dee"] {
            assert!(scores[quiet] < 10, "{scores:?}");
        }
    }

    #[test]
    fn someone_quiet_today_is_scored_back_down() {
        let mut days = month(0);
        days.remove(&0);
        let scored = score_all(&[(human("ana"), days)].into(), &Opened::new(), &HashMap::new());
        assert_eq!(scored[0].score.score, 0);
    }

    #[test]
    fn only_what_nobody_on_the_team_had_opened_is_unfamiliar() {
        let set = |ids: &[&str]| ids.iter().map(|id| human(id)).collect::<BTreeSet<_>>();
        let opened: Opened = [
            // Lee's own runbook, and one a teammate keeps: familiar.
            ("runbook".to_string(), Openers { before: set(&["lee"]), last_day: set(&["lee"]) }),
            ("backups".to_string(), Openers { before: set(&["kim"]), last_day: set(&["lee"]) }),
            // Opened by a teammate alongside him today: a new team document, familiar.
            ("new-doc".to_string(), Openers { before: set(&[]), last_day: set(&["lee", "kim"]) }),
            // Engineering's overview, opened only by engineers before: unfamiliar to Lee alone.
            ("overview".to_string(), Openers { before: set(&["eve"]), last_day: set(&["lee", "eve"]) }),
            ("glossary".to_string(), Openers { before: set(&[]), last_day: set(&["lee"]) }),
            // Everyone's handbook: too popular to say.
            (
                "handbook".to_string(),
                Openers {
                    before: (0..=MAX_OPENERS).map(|i| human(&format!("p{i}"))).collect(),
                    last_day: set(&["lee"]),
                },
            ),
        ]
        .into();
        let teams: HashMap<Principal, String> = [("lee", "it-admin"), ("kim", "it-admin"), ("eve", "engineering")]
            .map(|(p, t)| (human(p), t.to_string()))
            .into();
        let counts = unfamiliar(&opened, &teams);
        assert_eq!(counts.get(&human("lee")), Some(&2), "{counts:?}");
        assert_eq!(counts.get(&human("kim")), None);
        assert_eq!(counts.get(&human("eve")), None);
    }
}

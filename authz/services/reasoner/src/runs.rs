//! Investigations on the reasoner's flags (ADR 0071): who gets one this run, within the monthly
//! token budget, the case built from awzy's own records, and the outcome kept in awzy-context and
//! acted on through automated containments (ADR 0066).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use awzy_authz_proto::challenge::v1::ListChallengesRequest;
use awzy_llm::Models;
use chrono::{DateTime, Datelike, TimeZone, Utc};
use serde_json::{json, Value};

use crate::{
    activity::Principal,
    contain::{Containments, ELEVATED},
    context::ContextClient,
    investigate::{self, Case, Containment, Item, Outcome, Run, Source},
    Scored,
};

/// The most decisions, and events, one case shows.
const MOST_RECORDS: u64 = 60;
/// The most documents one case reads, and the most of each one's text.
const MOST_DOCUMENTS: usize = 5;
const MOST_TEXT: usize = 2_000;

/// How investigations run in this install.
pub struct Investigations<M> {
    /// Where each investigation gets its model: the install's router, by the `investigations`
    /// route (ADR 0114).
    pub models: M,
    /// The most tokens a calendar month may spend.
    pub monthly_tokens: u64,
    /// Groups whose members are only ever proposed for containment.
    pub privileged_groups: Vec<String>,
}

/// Someone to investigate this run, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Flagged {
    pub principal: Principal,
    pub score: i32,
    pub trigger: String,
}

/// Who to investigate, highest score first: an elevated score, or a challenge rejected in the last
/// day, and not investigated in the last day already.
pub fn flagged(
    scores: &[Scored],
    rejected: &BTreeMap<String, Vec<String>>,
    recent: &BTreeSet<Principal>,
) -> Vec<Flagged> {
    let mut out: Vec<Flagged> = scores
        .iter()
        .filter(|s| !recent.contains(&s.principal))
        .filter_map(|s| {
            let rejections = (s.principal.0 == "Human").then(|| rejected.get(&s.principal.1)).flatten();
            let mut why = Vec::new();
            if s.score.score > ELEVATED {
                why.push(format!("risk {} ({})", s.score.score, s.score.reasons.join("; ")));
            }
            if let Some(r) = rejections.filter(|r| !r.is_empty()) {
                why.push(format!("{} rejected challenge(s)", r.len()));
            }
            (!why.is_empty()).then(|| Flagged {
                principal: s.principal.clone(),
                score: s.score.score,
                trigger: why.join("; "),
            })
        })
        .collect();
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.principal.cmp(&b.principal)));
    out
}

/// The start of `as_of`'s calendar month, UTC: where the budget resets.
pub fn month_start(as_of: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(as_of.year(), as_of.month(), 1, 0, 0, 0).single().unwrap_or(as_of)
}

/// The record awzy-context keeps for one run.
pub fn record(f: &Flagged, model: &str, started_at: DateTime<Utc>, run: &Run) -> Value {
    let o: &Outcome = &run.outcome;
    let needs_review = o.needs_review || o.containment == Containment::Propose;
    json!({
        "principal": { "type": f.principal.0, "id": f.principal.1 },
        "trigger": f.trigger,
        "floor": f.score.clamp(0, 100),
        "status": if needs_review { "needs_review" } else { "done" },
        "verdict": o.verdict,
        "risk": o.risk,
        "summary": o.summary,
        "evidence": o.evidence,
        "containment": o.containment,
        "notes": o.notes,
        "finding": run.finding,
        "model": model,
        "input_tokens": run.usage.input_tokens,
        "output_tokens": run.usage.output_tokens,
        "started_at": started_at.to_rfc3339(),
    })
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct DecisionRow {
    id: String,
    at: String,
    on_behalf_of_type: String,
    on_behalf_of_id: String,
    action: String,
    resource_type: String,
    resource_id: String,
    verdict: String,
    mode: String,
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct EventRow {
    id: String,
    at: String,
    source: String,
    system: String,
    action: String,
    resource_type: String,
    resource_id: String,
    outcome: String,
}

const DECISIONS: &str = "
SELECT toString(id) AS id, toString(at) AS at, on_behalf_of_type, on_behalf_of_id, action, resource_type,
       resource_id, verdict, mode
FROM decisions
WHERE principal_type = ? AND principal_id = ?
  AND at > fromUnixTimestamp64Milli(?, 'UTC') AND at <= fromUnixTimestamp64Milli(?, 'UTC')
ORDER BY at DESC
LIMIT ?";

// The principal's own events, and its agents' on its behalf.
const EVENTS: &str = "
SELECT toString(id) AS id, toString(at) AS at, source, system, action, resource_type, resource_id, outcome
FROM events
WHERE ((principal_type = ? AND principal_id = ?) OR (on_behalf_of_type = ? AND on_behalf_of_id = ?))
  AND at > fromUnixTimestamp64Milli(?, 'UTC') AND at <= fromUnixTimestamp64Milli(?, 'UTC')
ORDER BY at DESC
LIMIT ?";

fn truncate(s: &str, most: usize) -> String {
    match s.char_indices().nth(most) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// The case for `f`: its last day of decisions, events, challenges, and the documents it opened.
pub async fn build_case(
    clickhouse: &clickhouse::Client,
    context: &ContextClient,
    containments: Option<&Containments>,
    f: &Flagged,
    privileged: bool,
    as_of: DateTime<Utc>,
) -> anyhow::Result<Case> {
    let (kind, id) = (&f.principal.0, &f.principal.1);
    let end = as_of.timestamp_millis();
    let start = (as_of - chrono::Duration::days(1)).timestamp_millis();
    let mut items = Vec::new();
    let mut opened: Vec<String> = Vec::new();

    let decisions: Vec<DecisionRow> =
        clickhouse.query(DECISIONS).bind(kind).bind(id).bind(start).bind(end).bind(MOST_RECORDS).fetch_all().await?;
    for d in decisions {
        let by = if d.on_behalf_of_id.is_empty() {
            String::new()
        } else {
            format!(" for {}::{}", d.on_behalf_of_type, d.on_behalf_of_id)
        };
        if d.resource_type == "Resource" && !opened.contains(&d.resource_id) {
            opened.push(d.resource_id.clone());
        }
        items.push(Item {
            id: format!("decision_{}", d.id),
            source: Source::Decision,
            at: d.at,
            summary: format!(
                "{kind}::{id}{by} {} {}::{}: {} ({})",
                d.action, d.resource_type, d.resource_id, d.verdict, d.mode
            ),
            untrusted: None,
        });
    }

    let events: Vec<EventRow> = clickhouse
        .query(EVENTS)
        .bind(kind)
        .bind(id)
        .bind(kind)
        .bind(id)
        .bind(start)
        .bind(end)
        .bind(MOST_RECORDS)
        .fetch_all()
        .await?;
    for e in events {
        let source = if e.source.contains("cloudtrail") { Source::CloudAudit } else { Source::ServerLog };
        items.push(Item {
            id: format!("event_{}", e.id),
            source,
            at: e.at,
            summary: format!(
                "{} in {}: {} {}::{} ({})",
                e.source, e.system, e.action, e.resource_type, e.resource_id, e.outcome
            ),
            untrusted: None,
        });
    }

    if let (Some(c), "Human") = (containments, kind.as_str()) {
        let since = start / 1000;
        let req = ListChallengesRequest { answerer_id: id.clone(), limit: 50, ..Default::default() };
        for ch in c.challenge.clone().list_challenges(req).await?.into_inner().challenges {
            if ch.created_at.as_ref().is_none_or(|t| t.seconds <= since) {
                continue;
            }
            let on = |e: &Option<awzy_authz_proto::authz::v1::EntityRef>| {
                e.as_ref().map(|e| format!("{}::{}", e.r#type, e.id)).unwrap_or_default()
            };
            let decided =
                if ch.decided_by.is_empty() { String::new() } else { format!(", decided by {}", ch.decided_by) };
            let note = if ch.decision_note.is_empty() {
                String::new()
            } else {
                format!("\nDecision note: {}", ch.decision_note)
            };
            items.push(Item {
                id: ch.qid.clone(),
                source: Source::Challenge,
                at: ch
                    .created_at
                    .and_then(|t| DateTime::from_timestamp(t.seconds, 0))
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default(),
                summary: format!(
                    "challenge for {} {} {}: status {}{decided}",
                    on(&ch.principal),
                    ch.action,
                    on(&ch.resource),
                    ch.status().as_str_name()
                ),
                untrusted: Some(format!("Justification: {}{note}", ch.justification)).filter(|t| t.len() > 15),
            });
        }
    }

    for resource in opened.iter().take(MOST_DOCUMENTS) {
        if let Some((name, text)) = context.resource(resource).await? {
            items.push(Item {
                id: resource.clone(),
                source: Source::Document,
                at: String::new(),
                summary: format!("the document {resource}, which {kind}::{id} opened"),
                untrusted: Some(truncate(&format!("{name}\n\n{text}"), MOST_TEXT)),
            });
        }
    }

    Ok(Case {
        principal: f.principal.clone(),
        floor: f.score.clamp(0, 100),
        trigger: f.trigger.clone(),
        privileged,
        items,
    })
}

/// Investigations' route in the install's model routing.
pub const FEATURE: &str = "investigations";

impl<M: Models> Investigations<M> {
    /// Investigates whoever this run flags, within the month's budget. Returns how many ran.
    pub async fn run(
        &self,
        clickhouse: &clickhouse::Client,
        context: &ContextClient,
        containments: Option<&Containments>,
        scores: &[Scored],
        rejected: &BTreeMap<String, Vec<String>>,
        as_of: DateTime<Utc>,
    ) -> anyhow::Result<usize> {
        let mut spent = context.investigation_tokens(month_start(as_of)).await?;
        if spent >= self.monthly_tokens {
            tracing::warn!(spent, budget = self.monthly_tokens, "investigation budget spent for this month");
            return Ok(0);
        }
        let recent = context.investigated_since(as_of - chrono::Duration::days(1)).await?;
        let to_run = flagged(scores, rejected, &recent);
        if to_run.is_empty() {
            return Ok(0);
        }
        let groups: HashMap<Principal, BTreeSet<String>> = crate::context::memberships(&context.snapshot().await?);
        let mut ran = 0;
        for f in to_run {
            if spent >= self.monthly_tokens {
                tracing::warn!(spent, budget = self.monthly_tokens, "investigation budget spent; the rest wait");
                break;
            }
            let privileged =
                groups.get(&f.principal).is_some_and(|g| self.privileged_groups.iter().any(|p| g.contains(p)));
            let started_at = Utc::now();
            let case = build_case(clickhouse, context, containments, &f, privileged, as_of).await?;
            let model = self.models.conversation(FEATURE)?;
            let run = investigate::investigate(&model, &case).await?;
            let before = spent;
            spent += run.usage.input_tokens + run.usage.output_tokens;
            if before * 5 < self.monthly_tokens * 4 && spent * 5 >= self.monthly_tokens * 4 {
                tracing::warn!(
                    spent,
                    budget = self.monthly_tokens,
                    "80 percent of this month's investigation budget is spent"
                );
            }
            let qid = context.put_investigation(record(&f, &run.model, started_at, &run)).await?;
            ran += 1;
            tracing::info!(investigation = %qid, principal = ?f.principal, verdict = ?run.outcome.verdict,
                containment = ?run.outcome.containment, "investigation finished");
            if let (Containment::Recommend, Some(c), "Human") =
                (run.outcome.containment, containments, f.principal.0.as_str())
            {
                let mode = c.mode().await?;
                let reason = format!("awzy-reasoner investigation {qid}: {}", run.outcome.summary);
                c.contain(&crate::contain::Proposal { human: f.principal.1.clone(), reason }, mode).await?;
                tracing::warn!(investigation = %qid, human = %f.principal.1, mode = mode.as_str(), "containment from an investigation");
            }
        }
        Ok(ran)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        investigate::{Cited, Finding, Verdict},
        score::Score,
    };

    fn scored(kind: &str, id: &str, score: i32) -> Scored {
        Scored {
            principal: (kind.into(), id.into()),
            score: Score { score, reasons: vec!["r".into()] },
            automation: None,
        }
    }

    #[test]
    fn the_flagged_are_the_elevated_and_the_rejected_riskiest_first_once_a_day() {
        let scores = [
            scored("Human", "quiet", 5),
            scored("Human", "nora", 50),
            scored("Human", "rejected", 10),
            scored("ServiceAccount", "genie", 90),
            scored("Human", "yesterday", 95),
        ];
        let rejected: BTreeMap<String, Vec<String>> =
            [("rejected".to_string(), vec!["one".to_string()]), ("genie".to_string(), vec!["x".to_string()])].into();
        let recent: BTreeSet<Principal> = [("Human".to_string(), "yesterday".to_string())].into();
        let who: Vec<(String, i32)> =
            flagged(&scores, &rejected, &recent).into_iter().map(|f| (f.principal.1, f.score)).collect();
        assert_eq!(who, vec![("genie".into(), 90), ("nora".into(), 50), ("rejected".into(), 10)]);
        let f = flagged(&scores, &rejected, &recent);
        assert!(f[2].trigger.contains("rejected challenge"), "{:?}", f[2]);
        assert!(!f[0].trigger.contains("rejected"), "a service account's rejections are its person's");
    }

    #[test]
    fn the_budget_resets_at_the_start_of_each_month() {
        let mid = Utc.with_ymd_and_hms(2026, 10, 17, 13, 5, 0).unwrap();
        assert_eq!(month_start(mid), Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap());
    }

    #[test]
    fn records_need_review_when_a_person_has_to_decide() {
        let f = Flagged { principal: ("Human".into(), "lee".into()), score: 50, trigger: "risk 50".into() };
        let outcome = |containment, needs_review| Outcome {
            verdict: Verdict::Malicious,
            risk: 80,
            summary: "s".into(),
            evidence: vec![Cited { id: "decision_1".into(), why: "w".into() }],
            containment,
            needs_review,
            notes: vec![],
        };
        let run = |o| Run {
            outcome: o,
            finding: Some(Finding {
                principal: "Human::lee".into(),
                verdict: Verdict::Malicious,
                risk: 80,
                summary: "s".into(),
                evidence: vec![],
                contain: true,
            }),
            tool_calls: vec![],
            usage: awzy_llm::Usage { input_tokens: 1200, output_tokens: 300 },
            model: "m".into(),
        };
        let proposed = record(&f, "m", Utc::now(), &run(outcome(Containment::Propose, false)));
        assert_eq!(
            (proposed["status"].as_str(), proposed["containment"].as_str()),
            (Some("needs_review"), Some("propose"))
        );
        assert_eq!((proposed["verdict"].as_str(), proposed["input_tokens"].as_u64()), (Some("malicious"), Some(1200)));
        let done = record(&f, "m", Utc::now(), &run(outcome(Containment::Recommend, false)));
        assert_eq!(done["status"], "done");
    }

    #[test]
    fn long_text_is_cut_at_a_character() {
        assert_eq!(truncate("héllo", 2), "hé…");
        assert_eq!(truncate("hi", 5), "hi");
    }
}

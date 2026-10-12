//! The policy dry-run (ADR 0049): what a draft bundle would change, from the requests the PDPs
//! decided recently.
//!
//! Distinct recent requests are replayed under the active bundle and the draft, both with
//! today's people, groups, and resources from awzy-context, and the verdicts compared. It runs
//! here, on request, never on the PDPs serving traffic.

use std::collections::BTreeMap;

use async_graphql::{Context, Object, Result, SimpleObject};
use awzy_model::{Check, Decision, DecisionId, EntityRef, PolicyBundleId, Verdict};
use awzy_pdp::{replay::Replayer, Pdp};
use awzy_portal::Qid;
use chrono::{Duration, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    access::{require, PortalAction, PortalArea},
    policies, Services,
};

pub const DEFAULT_DAYS: u32 = 7;
pub const MAX_DAYS: u32 = 30;
/// Distinct requests replayed, most frequent first.
pub const MAX_REQUESTS: u64 = 5_000;
const EXAMPLES: usize = 5;

/// One distinct request from the decision log, and how many times it was decided.
#[derive(Debug, Clone, clickhouse::Row, Deserialize)]
pub struct RequestGroup {
    pub principal_type: String,
    pub principal_id: String,
    pub on_behalf_of_type: String,
    pub on_behalf_of_id: String,
    pub human_present: bool,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub context: String,
    /// The work item the request named (ADR 0118); empty when none.
    pub work_item_id: String,
    pub decisions: u64,
    pub sample_id: String,
}

const GROUP_BY: &str =
    "principal_type, principal_id, on_behalf_of_type, on_behalf_of_id, human_present, action, resource_type, resource_id, context, work_item_id";

/// The most frequent distinct requests since `days` ago, how many decisions there were in all,
/// and how many distinct requests.
pub async fn recent_requests(
    ch: &::clickhouse::Client,
    days: u32,
    limit: u64,
) -> anyhow::Result<(Vec<RequestGroup>, u64, u64)> {
    let since = (Utc::now() - Duration::days(i64::from(days))).timestamp_millis();
    let (total, distinct): (u64, u64) = ch
        .query(&format!(
            "SELECT count(), uniqExact(tuple({GROUP_BY})) FROM decisions WHERE at >= fromUnixTimestamp64Milli(?, 'UTC')"
        ))
        .bind(since)
        .fetch_one()
        .await?;
    let groups = ch
        .query(&format!(
            "SELECT {GROUP_BY}, count() AS decisions, toString(any(id)) AS sample_id
             FROM decisions WHERE at >= fromUnixTimestamp64Milli(?, 'UTC')
             GROUP BY {GROUP_BY} ORDER BY decisions DESC LIMIT ?"
        ))
        .bind(since)
        .bind(limit)
        .fetch_all()
        .await?;
    Ok((groups, total, distinct))
}

/// How many query plans the PDPs made since `days` ago.
pub async fn recent_plans(ch: &::clickhouse::Client, days: u32) -> anyhow::Result<u64> {
    let since = (Utc::now() - Duration::days(i64::from(days))).timestamp_millis();
    Ok(ch
        .query("SELECT count() FROM plans WHERE at >= fromUnixTimestamp64Milli(?, 'UTC')")
        .bind(since)
        .fetch_one()
        .await?)
}

fn request_of(g: &RequestGroup) -> anyhow::Result<Check> {
    let context = match g.context.trim() {
        "" => Default::default(),
        c => serde_json::from_str(c)?,
    };
    Ok(Check {
        principal: EntityRef::new(&g.principal_type, &g.principal_id),
        action: g.action.clone(),
        resource: EntityRef::new(&g.resource_type, &g.resource_id),
        context,
        on_behalf_of: (!g.on_behalf_of_id.is_empty()).then(|| EntityRef::new(&g.on_behalf_of_type, &g.on_behalf_of_id)),
        human_present: Some(g.human_present),
        always_enforced: false,
        work_item_id: (!g.work_item_id.is_empty()).then(|| g.work_item_id.clone()),
    })
}

/// allow, deny, or challenge with its kinds, e.g. "challenge (approval)".
fn verdict_label(d: &Decision) -> String {
    match &d.verdict {
        Verdict::Challenge { challenges } => format!("challenge ({})", challenges.joined().replace(',', ", ")),
        v => v.as_str().to_string(),
    }
}

#[derive(SimpleObject, Clone)]
pub struct PolicyDryRunExample {
    pub principal_type: String,
    pub principal_id: String,
    /// The person a service account acted for, if any.
    pub on_behalf_of_id: Option<String>,
    pub action: String,
    pub resource_id: String,
    /// How many times this exact request was decided in the window.
    pub decisions: u64,
    /// One of those decisions, `decision_<uuid>`.
    /// Null if the sample's ID in ClickHouse isn't a UUID.
    pub sample_decision_qid: Option<Qid<DecisionId>>,
    /// The policies that decided it under the active bundle, and under the draft.
    pub policies_before: Vec<String>,
    pub policies_after: Vec<String>,
}

#[derive(SimpleObject, Clone)]
pub struct PolicyDryRunChange {
    /// The verdict under the active bundle: allow, deny, or challenge (with its kinds).
    pub from: String,
    /// The verdict under the draft.
    pub to: String,
    /// Decisions in the window that would have come out differently.
    pub decisions: u64,
    /// Distinct requests among them.
    pub requests: u64,
    /// The most frequent ones.
    pub examples: Vec<PolicyDryRunExample>,
}

#[derive(SimpleObject)]
pub struct PolicyDryRun {
    pub days: u32,
    /// The active bundle's version the draft was compared with; 0 for the built-in defaults.
    pub compared_with_version: i64,
    /// Decisions in the window, all of them.
    pub decisions: u64,
    /// Distinct requests among them.
    pub distinct_requests: u64,
    /// Distinct requests replayed: the most frequent, up to the limit.
    pub replayed_requests: u64,
    /// Fewer were replayed than there were.
    pub truncated: bool,
    /// Decisions whose verdict the draft leaves as it is.
    pub unchanged_decisions: u64,
    /// Changes of verdict, most decisions first.
    pub changes: Vec<PolicyDryRunChange>,
    /// Requests that couldn't be replayed, and why the first one couldn't.
    pub skipped_requests: u64,
    pub skipped_reason: Option<String>,
    /// Query plans in the window (ADR 0113). A plan decides no one resource, so a dry run
    /// doesn't replay them; it says how many there were.
    pub plans_not_replayed: u64,
}

/// Compares `draft` with `active` over `groups`.
pub async fn compare(
    replayer: &Replayer,
    active: &Pdp,
    draft: &Pdp,
    groups: &[RequestGroup],
) -> (u64, Vec<PolicyDryRunChange>, u64, Option<String>) {
    let mut unchanged = 0;
    let mut skipped = 0;
    let mut skipped_reason = None;
    let mut changes: BTreeMap<(String, String), PolicyDryRunChange> = BTreeMap::new();
    for g in groups {
        let outcome = match request_of(g) {
            Ok(req) => replayer.compare(active, draft, &req).await,
            Err(e) => Err(e),
        };
        let (before, after) = match outcome {
            Ok(pair) => pair,
            Err(e) => {
                skipped += 1;
                skipped_reason.get_or_insert_with(|| format!("{} on {}: {e}", g.principal_id, g.resource_id));
                continue;
            }
        };
        let (from, to) = (verdict_label(&before), verdict_label(&after));
        if from == to {
            unchanged += g.decisions;
            continue;
        }
        let change = changes.entry((from.clone(), to.clone())).or_insert_with(|| PolicyDryRunChange {
            from,
            to,
            decisions: 0,
            requests: 0,
            examples: vec![],
        });
        change.decisions += g.decisions;
        change.requests += 1;
        // Groups arrive most frequent first, so the first few are the examples.
        if change.examples.len() < EXAMPLES {
            change.examples.push(PolicyDryRunExample {
                principal_type: g.principal_type.clone(),
                principal_id: g.principal_id.clone(),
                on_behalf_of_id: (!g.on_behalf_of_id.is_empty()).then(|| g.on_behalf_of_id.clone()),
                action: g.action.clone(),
                resource_id: g.resource_id.clone(),
                decisions: g.decisions,
                sample_decision_qid: Uuid::parse_str(&g.sample_id).ok().map(|u| Qid(DecisionId::from_uuid(u))),
                policies_before: before.policies,
                policies_after: after.policies,
            });
        }
    }
    let mut changes: Vec<_> = changes.into_values().collect();
    changes.sort_by_key(|c| std::cmp::Reverse(c.decisions));
    (unchanged, changes, skipped, skipped_reason)
}

#[derive(Default)]
pub struct DryRunQuery;

#[Object]
impl DryRunQuery {
    /// What a draft bundle would change (ADR 0049): recent requests replayed under the active
    /// bundle and the draft, with today's people and resources, and the verdicts compared.
    /// `days` defaults to 7, at most 30.
    async fn policy_dry_run(
        &self,
        ctx: &Context<'_>,
        draft_qid: Qid<PolicyBundleId>,
        days: Option<u32>,
    ) -> Result<PolicyDryRun> {
        require(ctx, PortalAction::View, PortalArea::Policies).await?;
        require(ctx, PortalAction::View, PortalArea::Decisions).await?;
        Ok(run(ctx.data::<Services>()?, &draft_qid.to_string(), days).await?)
    }
}

/// A dry run as JSON, for the MCP tool.
pub(crate) fn to_json(r: &PolicyDryRun) -> serde_json::Value {
    let example = |e: &PolicyDryRunExample| {
        serde_json::json!({
            "principal": format!("{}::{}", e.principal_type, e.principal_id),
            "on_behalf_of": e.on_behalf_of_id,
            "action": e.action,
            "resource_id": e.resource_id,
            "decisions": e.decisions,
            "sample_decision_qid": e.sample_decision_qid.map(|q| q.to_string()),
            "policies_before": e.policies_before,
            "policies_after": e.policies_after,
        })
    };
    serde_json::json!({
        "days": r.days,
        "compared_with_version": r.compared_with_version,
        "decisions": r.decisions,
        "distinct_requests": r.distinct_requests,
        "replayed_requests": r.replayed_requests,
        "truncated": r.truncated,
        "unchanged_decisions": r.unchanged_decisions,
        "changes": r.changes.iter().map(|c| serde_json::json!({
            "from": c.from, "to": c.to, "decisions": c.decisions, "requests": c.requests,
            "examples": c.examples.iter().map(example).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "skipped_requests": r.skipped_requests,
        "skipped_reason": r.skipped_reason,
        "plans_not_replayed": r.plans_not_replayed,
    })
}

/// The replayer a dry run or a what-if check decides with: today's people and resources from
/// awzy-context when the portal has it, else only what the request carries.
pub(crate) async fn replayer(services: &Services) -> anyhow::Result<Replayer> {
    Ok(match &services.context {
        Some(context) => Replayer::from_context(context.base()).await?,
        None => Replayer::without_context(),
    })
}

/// A dry run of draft `draft_qid` over the last `days` (default 7, at most 30). The caller has
/// checked the viewer may see policies and decisions.
pub(crate) async fn run(services: &Services, draft_qid: &str, days: Option<u32>) -> anyhow::Result<PolicyDryRun> {
    let days = days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS);
    let policy = services
        .policy
        .clone()
        .ok_or_else(|| anyhow::anyhow!("the policy service is not configured (AWZY_POLICY_ADDR)"))?;
    let ch = services
        .clickhouse
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("the decision log isn't configured (AWZY_CLICKHOUSE_URL)"))?;
    let id: PolicyBundleId = draft_qid.parse()?;
    let draft = policies::get(policy.clone(), id).await?.ok_or_else(|| anyhow::anyhow!("no such policy bundle"))?;
    let draft = Pdp::new(&draft.schema, &draft.policies)?;
    let (active, compared_with_version) = match policies::active(policy).await? {
        Some(b) => (Pdp::new(&b.schema, &b.policies)?, b.version),
        None => (Pdp::with_defaults()?, 0),
    };
    let replayer = replayer(services).await?;
    let (groups, decisions, distinct) = recent_requests(ch, days, MAX_REQUESTS).await?;
    let (unchanged, changes, skipped, skipped_reason) = compare(&replayer, &active, &draft, &groups).await;
    let plans_not_replayed = recent_plans(ch, days).await?;
    Ok(PolicyDryRun {
        days,
        compared_with_version,
        decisions,
        distinct_requests: distinct,
        replayed_requests: groups.len() as u64,
        truncated: distinct > groups.len() as u64,
        unchanged_decisions: unchanged,
        changes,
        skipped_requests: skipped,
        skipped_reason,
        plans_not_replayed,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use awzy_pdp::{principals::Principals, resources::Resources};
    use serde_json::json;

    use super::*;
    use crate::policies::tests::{serve_fake_policy, FakePolicy};

    fn group(principal: &str, resource: &str, decisions: u64, context: &str) -> RequestGroup {
        RequestGroup {
            principal_type: "Human".into(),
            principal_id: principal.into(),
            on_behalf_of_type: String::new(),
            on_behalf_of_id: String::new(),
            human_present: true,
            action: "read".into(),
            resource_type: "Resource".into(),
            resource_id: resource.into(),
            context: context.into(),
            work_item_id: String::new(),
            decisions,
            sample_id: Uuid::now_v7().to_string(),
        }
    }

    fn replayer() -> Replayer {
        let principals = Arc::new(Principals::awaiting_sync());
        principals.load(
            1,
            vec![
                json!({ "uid": { "type": "Human", "id": "alice" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "security" }] }),
                json!({ "uid": { "type": "Human", "id": "carol" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "security" }] }),
                json!({ "uid": { "type": "Human", "id": "bob" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "engineering" }] }),
            ],
        );
        Replayer::new(principals, Arc::new(Resources::none()))
    }

    fn no_security_reads() -> Pdp {
        Pdp::new(
            awzy_pdp::DEFAULT_SCHEMA,
            &format!("{}\n@id(\"no-security-reads\")\nforbid (principal in Group::\"security\", action == Action::\"read\", resource);", awzy_pdp::DEFAULT_POLICIES),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn changes_are_counted_by_verdict_with_the_most_frequent_examples() {
        let groups = [
            group("alice", "board-deck", 40, ""),
            group("carol", "q3-plan", 2, r#"{"risk_score": 10}"#),
            group("bob", "design-doc", 7, ""),
            group("alice", "broken", 1, "not json"),
        ];
        let (unchanged, changes, skipped, reason) =
            compare(&replayer(), &Pdp::with_defaults().unwrap(), &no_security_reads(), &groups).await;
        assert_eq!(changes.len(), 1);
        let c = &changes[0];
        assert_eq!((c.from.as_str(), c.to.as_str(), c.decisions, c.requests), ("allow", "deny", 42, 2));
        assert_eq!(c.examples[0].principal_id, "alice", "most frequent first");
        assert_eq!(c.examples[0].policies_before, ["security-team-read"]);
        assert_eq!(c.examples[0].policies_after, ["no-security-reads"]);
        assert!(c.examples[0].sample_decision_qid.is_some(), "a UUID from ClickHouse is a decision QID");
        assert_eq!(unchanged, 7, "bob isn't in security");
        assert_eq!(skipped, 1);
        assert!(reason.unwrap().contains("broken"));
    }

    /// With --config=local-stack: recent decisions are grouped from ClickHouse, and a dry-run
    /// runs end to end through GraphQL.
    #[tokio::test]
    async fn recent_decisions_are_grouped_and_a_dry_run_reports_them() {
        let Ok(url) = std::env::var("AWZY_TEST_CLICKHOUSE_URL") else {
            eprintln!("skipping: AWZY_TEST_CLICKHOUSE_URL is not set");
            return;
        };
        let database = format!("test_{}", Uuid::now_v7().simple());
        let admin = ::clickhouse::Client::default().with_url(&url).with_user("awzy").with_password("awzy");
        admin.query(&format!("CREATE DATABASE {database}")).execute().await.unwrap();
        let ch = admin.clone().with_database(&database);
        let settings = awzy_clickhouse::Settings {
            kafka_brokers: "localhost:1".into(),
            decisions_topic: format!("unused.{database}"),
            events_topic: format!("unused-events.{database}"),
            plans_topic: format!("unused-plans.{database}"),
            consumer_group: database.clone(),
        };
        awzy_clickhouse::migrate(&ch, &settings).await.unwrap();
        // alice read board-deck 3 times and bob design-doc once in the window; carol 40 days ago.
        let now = Utc::now().timestamp_millis();
        let row = |principal: &str, resource: &str, at: i64| {
            format!(
                "('{}', fromUnixTimestamp64Milli({at}, 'UTC'), 'Human', '{principal}', 'read', 'Resource', '{resource}', 'allow', [], true, '')",
                Uuid::now_v7()
            )
        };
        let day = 86_400_000;
        let rows = [
            row("alice", "board-deck", now - 1000),
            row("alice", "board-deck", now - 2000),
            row("alice", "board-deck", now - 2 * day),
            row("bob", "design-doc", now - 3000),
            row("carol", "q3-plan", now - 40 * day),
        ];
        ch.query(&format!(
            "INSERT INTO decisions (id, at, principal_type, principal_id, action, resource_type, resource_id, verdict, policies, human_present, context) VALUES {}",
            rows.join(",")
        ))
        .execute()
        .await
        .unwrap();

        let (groups, total, distinct) = recent_requests(&ch, 7, 10).await.unwrap();
        assert_eq!((total, distinct), (4, 2));
        assert_eq!((groups[0].principal_id.as_str(), groups[0].decisions), ("alice", 3));
        let (groups, _, _) = recent_requests(&ch, 7, 1).await.unwrap();
        assert_eq!(groups.len(), 1, "the limit keeps the most frequent");

        let fake = FakePolicy::default();
        let schema = crate::schema_with(Services {
            policy: Some(serve_fake_policy(fake).await),
            clickhouse: Some(ch),
            pdp: Some(crate::testing::pdp().await),
            ..Default::default()
        });
        let alice = crate::testing::human("alice", "security");
        let draft = crate::testing::run(
            &schema,
            &alice,
            format!(
                "mutation {{ createPolicyDraft(input: {{ schema: {}, policies: {} }}) {{ bundle {{ id }} }} }}",
                serde_json::to_string(awzy_pdp::DEFAULT_SCHEMA).unwrap(),
                serde_json::to_string(awzy_pdp::DEFAULT_POLICIES).unwrap()
            ),
        )
        .await;
        let id = draft["createPolicyDraft"]["bundle"]["id"].as_str().unwrap();
        let out = crate::testing::run(
            &schema,
            &alice,
            format!(r#"{{ policyDryRun(draftQid: "{id}") {{ days comparedWithVersion decisions distinctRequests replayedRequests truncated unchangedDecisions skippedRequests changes {{ from to }} }} }}"#),
        )
        .await;
        admin.query(&format!("DROP DATABASE {database}")).execute().await.unwrap();
        // The draft is the built-in policies, which are also what's in force: nothing changes.
        assert_eq!(
            out["policyDryRun"],
            json!({ "days": 7, "comparedWithVersion": 0, "decisions": 4, "distinctRequests": 2, "replayedRequests": 2,
                    "truncated": false, "unchangedDecisions": 4, "skippedRequests": 0, "changes": [] })
        );
    }
}

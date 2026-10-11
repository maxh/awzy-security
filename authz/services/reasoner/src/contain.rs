//! Automated containments (ADR 0066): a person is contained when, in the same day, their risk is
//! elevated, a challenge sent to them was rejected, and they or an agent acting for them tried to
//! move data out. Under the `automated-containments` scope's mode: in monitor, the containment is
//! recorded and not applied.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use awzy_authz_proto::{
    authz::v1::EnforcementMode as WireMode,
    challenge::v1::{
        challenge_service_client::ChallengeServiceClient, ChallengeStatus, ContainRequest, ListChallengesRequest,
        ListContainmentsRequest,
    },
    policy::v1::{policy_service_client::PolicyServiceClient, GetEnforcementRequest},
};
use awzy_model::{EnforcementMode, EntityRef};
use awzy_service_auth::AuthChannel;
use chrono::{DateTime, Utc};

use crate::Scored;

/// Scores above this are elevated: the same line the base policies draw for high-risk operations.
pub const ELEVATED: i32 = 40;

/// Event actions that move data out of the client's systems.
pub const EXFILTRATION_ACTIONS: &[&str] = &["download", "export", "send_external", "share_external"];

/// The most rejected challenges and containments read per run. A day with more is a day for people
/// to look at, not for this rule.
const MOST: i32 = 500;

/// What each person did in the last day that counts toward containing them, by user name.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Evidence {
    /// Rejected challenges sent to them, described.
    pub rejected: BTreeMap<String, Vec<String>>,
    /// Their attempts to move data out, and their agents', described.
    pub exfiltration: BTreeMap<String, Vec<String>>,
    /// People with an active containment, or one lifted in the last day: someone has already
    /// decided about them.
    pub decided: BTreeSet<String>,
}

/// A person to contain, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub human: String,
    pub reason: String,
}

/// Who to contain, from this run's scores and the day's evidence.
pub fn proposals(scores: &[Scored], evidence: &Evidence) -> Vec<Proposal> {
    scores
        .iter()
        .filter(|s| s.principal.0 == "Human" && s.score.score > ELEVATED)
        .filter_map(|s| {
            let human = &s.principal.1;
            if evidence.decided.contains(human) {
                return None;
            }
            let rejected = evidence.rejected.get(human).filter(|r| !r.is_empty())?;
            let exfiltration = evidence.exfiltration.get(human).filter(|e| !e.is_empty())?;
            let reason = format!(
                "awzy-reasoner, in the last day: risk {} ({}); {}; {}",
                s.score.score,
                s.score.reasons.join("; "),
                rejected.join("; "),
                exfiltration.join("; "),
            );
            Some(Proposal { human: human.clone(), reason })
        })
        .collect()
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct ExfiltrationRow {
    human: String,
    agent: String,
    action: String,
    resource_id: String,
    system: String,
}

// A person's own events, and their agents' on their behalf.
const EXFILTRATION_QUERY: &str = "
SELECT
    if(on_behalf_of_type = 'Human', on_behalf_of_id, principal_id) AS human,
    if(on_behalf_of_type = 'Human', principal_id, '') AS agent,
    action,
    resource_id,
    system
FROM events
WHERE at > fromUnixTimestamp64Milli(?, 'UTC') AND at <= fromUnixTimestamp64Milli(?, 'UTC')
  AND has(?, action)
  AND (principal_type = 'Human' OR on_behalf_of_type = 'Human')
ORDER BY at
LIMIT 10000";

/// Each person's attempts to move data out in the day before `as_of`.
pub async fn load_exfiltration(
    clickhouse: &clickhouse::Client,
    as_of: DateTime<Utc>,
) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    let end = as_of.timestamp_millis();
    let start = (as_of - chrono::Duration::days(1)).timestamp_millis();
    let rows: Vec<ExfiltrationRow> =
        clickhouse.query(EXFILTRATION_QUERY).bind(start).bind(end).bind(EXFILTRATION_ACTIONS).fetch_all().await?;
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for r in rows {
        let by = if r.agent.is_empty() { String::new() } else { format!(" through {}", r.agent) };
        out.entry(r.human).or_default().push(format!("{} of {} in {}{by}", r.action, r.resource_id, r.system));
    }
    Ok(out)
}

/// awzy-challenge and awzy-policy, with this service's tokens (ADR 0055).
pub struct Containments {
    pub challenge: ChallengeServiceClient<AuthChannel>,
    pub policy: PolicyServiceClient<AuthChannel>,
}

impl Containments {
    pub fn connect(challenge_addr: &str, policy_addr: &str) -> anyhow::Result<Self> {
        let channel = |addr: &str, audience: &str| -> anyhow::Result<AuthChannel> {
            let channel = tonic::transport::Channel::from_shared(addr.to_string())?.connect_lazy();
            Ok(awzy_service_auth::grpc_channel(channel, audience))
        };
        Ok(Self {
            challenge: ChallengeServiceClient::new(channel(challenge_addr, "awzy-challenge")?),
            policy: PolicyServiceClient::new(channel(policy_addr, "awzy-policy")?),
        })
    }

    /// The `automated-containments` scope's mode.
    pub async fn mode(&self) -> anyhow::Result<EnforcementMode> {
        let req = GetEnforcementRequest { scope: awzy_model::AUTOMATED_CONTAINMENTS_SCOPE.into() };
        let setting = self.policy.clone().get_enforcement(req).await?.into_inner().setting;
        let mode = setting.context("awzy-policy returned no enforcement setting")?.mode();
        Ok(if mode == WireMode::Enforce { EnforcementMode::Enforce } else { EnforcementMode::Monitor })
    }

    /// Rejected challenges sent to each person in the day before `as_of`, and who has already been
    /// decided about. Attempts to move data out come from ClickHouse ([`load_exfiltration`]).
    pub async fn evidence(&self, as_of: DateTime<Utc>) -> anyhow::Result<Evidence> {
        let since = (as_of - chrono::Duration::days(1)).timestamp();
        let req = ListChallengesRequest { status: ChallengeStatus::Rejected.into(), limit: MOST, ..Default::default() };
        let mut rejected: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for c in self.challenge.clone().list_challenges(req).await?.into_inner().challenges {
            if c.answerer_id.is_empty() || c.created_at.as_ref().is_none_or(|t| t.seconds <= since) {
                continue;
            }
            let label = |e: &Option<awzy_authz_proto::authz::v1::EntityRef>| {
                e.as_ref().map(|e| format!("{}::{}", e.r#type, e.id)).unwrap_or_default()
            };
            let by = if c.decided_by.is_empty() { String::new() } else { format!(" by {}", c.decided_by) };
            rejected.entry(c.answerer_id.clone()).or_default().push(format!(
                "{} {} {} rejected{by} ({})",
                label(&c.principal),
                c.action,
                label(&c.resource),
                c.qid
            ));
        }

        let req =
            ListContainmentsRequest { active_only: false, limit: MOST, include_monitor: true, ..Default::default() };
        let mut decided = BTreeSet::new();
        for c in self.challenge.clone().list_containments(req).await?.into_inner().containments {
            let Some(principal) = c.principal.filter(|s| s.r#type == "Human") else { continue };
            let lifted_recently = c.lifted_at.as_ref().is_some_and(|t| t.seconds > since);
            if c.lifted_at.is_none() || lifted_recently {
                decided.insert(principal.id);
            }
        }
        Ok(Evidence { rejected, exfiltration: BTreeMap::new(), decided })
    }

    /// Contains `p`, or records that it would have when `mode` is monitor.
    pub async fn contain(&self, p: &Proposal, mode: EnforcementMode) -> anyhow::Result<()> {
        let req = ContainRequest {
            principal: Some(EntityRef::new("Human", p.human.as_str()).into()),
            reason: p.reason.clone(),
            created_by_actor: Some(awzy_model::Actor::awzy("awzy-reasoner").into()),
            monitor: mode == EnforcementMode::Monitor,
            ..Default::default()
        };
        self.challenge.clone().contain(req).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::Score;

    fn scored(kind: &str, id: &str, score: i32) -> Scored {
        Scored {
            principal: (kind.into(), id.into()),
            score: Score { score, reasons: vec!["opened 7 resources that neither it nor its team had".into()] },
        }
    }

    fn evidence(humans: &[&str]) -> Evidence {
        let of = |what: &str| humans.iter().map(|p| (p.to_string(), vec![format!("{what} for {p}")])).collect();
        Evidence { rejected: of("a rejected challenge"), exfiltration: of("an export"), decided: BTreeSet::new() }
    }

    #[test]
    fn it_takes_all_three_to_contain_someone() {
        let scores = [scored("Human", "nora", 50)];
        let all = proposals(&scores, &evidence(&["nora"]));
        assert_eq!(all.len(), 1);
        assert!(all[0].reason.contains("risk 50") && all[0].reason.contains("an export for nora"), "{all:?}");

        let mut no_export = evidence(&["nora"]);
        no_export.exfiltration.clear();
        assert!(proposals(&scores, &no_export).is_empty(), "no attempt to move data out");
        let mut no_rejection = evidence(&["nora"]);
        no_rejection.rejected.clear();
        assert!(proposals(&scores, &no_rejection).is_empty(), "no rejected challenge");
        assert!(proposals(&[scored("Human", "nora", ELEVATED)], &evidence(&["nora"])).is_empty(), "risk not elevated");
    }

    #[test]
    fn only_people_and_only_once_a_day() {
        assert!(proposals(&[scored("ServiceAccount", "sales-genie", 90)], &evidence(&["sales-genie"])).is_empty());
        let mut reviewed = evidence(&["nora"]);
        reviewed.decided.insert("nora".into());
        assert!(proposals(&[scored("Human", "nora", 90)], &reviewed).is_empty(), "already contained or lifted today");
    }
}

//! Deciding a request the way a serving PDP would, outside one: for the policy dry-run
//! (ADR 0049), which replays recent requests under the active bundle and a draft.
//!
//! [`assemble`] is the one place that turns a request into Cedar entities. Live checks
//! (`AppState::check`) and replays both go through it, so a replay sees what a PDP would.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use awzy_model::{Check, Decision, DecisionId, EnforcementMode, Verdict};
use serde_json::Value;

use crate::{
    principals::{self, Principals},
    resources::Resources,
    Pdp,
};

/// A request's entities, ready to evaluate.
pub(crate) enum Assembled {
    /// A service account awzy-context doesn't know: denied without evaluating policy (ADR 0033).
    Unregistered,
    Ready {
        entities: Value,
        assignees: Option<Arc<HashSet<String>>>,
        /// awzy-reasoner's scores for the principal and the person it acts for (ADR 0056).
        risk: std::collections::HashMap<String, i64>,
        /// And their automation scores (ADR 0085).
        automation: std::collections::HashMap<String, i64>,
        /// Where the resource's attributes came from (ADR 0108).
        attributes_from: awzy_model::AttributesFrom,
    },
}

/// Who counts as assigned when `req` names the work item it's for (ADR 0118): nobody unless that
/// item is open and links the resource. Then its assignees; and for a delegated request, the
/// agent and the person both when the person is on it, so an agent works within the task it was
/// given rather than everything the person is assigned.
pub(crate) fn on_work_item(req: &Check, work: &HashMap<String, HashSet<String>>) -> HashSet<String> {
    let Some(on_item) = req.work_item_id.as_ref().and_then(|w| work.get(w)) else { return HashSet::new() };
    match &req.on_behalf_of {
        Some(human) if on_item.contains(&human.id) => HashSet::from([req.principal.id.clone(), human.id.clone()]),
        Some(_) => HashSet::new(),
        None => on_item.clone(),
    }
}

/// The person an `approve` check answers for, which awzy-challenge sets in its context.
fn approving_for(req: &Check) -> Option<awzy_model::EntityRef> {
    if req.action != "approve" {
        return None;
    }
    let id = req.context.get("approving_for")?.pointer("/__entity/id")?.as_str()?;
    Some(awzy_model::EntityRef::new(awzy_model::entity_type::HUMAN, id))
}

/// The request's inline entities plus its principals from the snapshot and its resource from
/// awzy-context. Fails only when awzy-context can't be reached for the resource.
pub(crate) async fn assemble(
    principals: &Principals,
    resources: &Resources,
    req: &Check,
    inline: Value,
) -> anyhow::Result<Assembled> {
    if principals.is_unregistered(&req.principal) {
        return Ok(Assembled::Unregistered);
    }
    let who: Vec<&awzy_model::EntityRef> = std::iter::once(&req.principal).chain(req.on_behalf_of.as_ref()).collect();
    // On an approval, the person it's for too, so policies can read their manager (ADR 0117).
    // Only their entity: risk and containment stay the approver's.
    let approving_for = approving_for(req);
    let loaded: Vec<&awzy_model::EntityRef> = who.iter().copied().chain(approving_for.as_ref()).collect();
    let entities = principals.merge(inline, &loaded);
    let merged = resources.merge(entities, &req.resource).await?;
    let risk = principals.risk_of(&who);
    let automation = principals.automation_of(&who);
    // A request that names its work item is assigned by that item alone (ADR 0118).
    let assignees = match (&req.work_item_id, merged.assignees) {
        (Some(_), Some(_)) => Some(Arc::new(on_work_item(req, &merged.work))),
        (_, assignees) => assignees,
    };
    Ok(Assembled::Ready { entities: merged.entities, assignees, risk, automation, attributes_from: merged.from })
}

/// The decision for a service account awzy-context doesn't know.
pub(crate) fn unregistered() -> Decision {
    Decision {
        id: DecisionId::new(),
        verdict: Verdict::Deny,
        policies: vec![principals::UNREGISTERED.to_string()],
        on_behalf_of: None,
        mode: EnforcementMode::Monitor,
        challenge_id: None,
    }
}

fn evaluate(pdp: &Pdp, req: &Check, assembled: &Assembled) -> anyhow::Result<Decision> {
    match assembled {
        Assembled::Unregistered => Ok(unregistered()),
        Assembled::Ready { entities, assignees, risk, automation, .. } => {
            let facts =
                crate::Facts { assignees: assignees.as_deref(), risk: Some(risk), automation: Some(automation) };
            pdp.decide_with(req, &pdp.entities(entities.clone())?, &facts, &Default::default())
        }
    }
}

/// Decides requests against any bundle with the entities a serving PDP would use.
pub struct Replayer {
    principals: Arc<Principals>,
    resources: Arc<Resources>,
}

impl Replayer {
    pub fn new(principals: Arc<Principals>, resources: Arc<Resources>) -> Self {
        Self { principals, resources }
    }

    /// With awzy-context: one principals snapshot, taken now, and resources looked up by ID.
    pub async fn from_context(endpoint: &str) -> anyhow::Result<Self> {
        let endpoint = endpoint.trim_end_matches('/');
        let principals = Arc::new(Principals::awaiting_sync());
        let client: awzy_http::Client = awzy_http::client();
        principals::sync_once(&principals, endpoint, &client, &awzy_service_auth::TokenSource::from_env()).await?;
        Ok(Self::new(principals, Arc::new(Resources::from_context(endpoint.to_string()))))
    }

    /// Without awzy-context, as a PDP without it decides: only the request's own entities.
    pub fn without_context() -> Self {
        Self::new(Arc::new(Principals::none()), Arc::new(Resources::none()))
    }

    /// The request decided under `pdp`, with the entities the caller already has (`inline`, in
    /// Cedar's entity JSON) besides what awzy-context knows: for asking how a draft would treat
    /// someone signed in now, from their token's entities.
    pub async fn decide(&self, pdp: &Pdp, req: &Check, inline: Value) -> anyhow::Result<Decision> {
        let assembled = assemble(&self.principals, &self.resources, req, inline).await?;
        evaluate(pdp, req, &assembled)
    }

    /// The request decided under `before` and under `after`, on the same entities.
    pub async fn compare(&self, before: &Pdp, after: &Pdp, req: &Check) -> anyhow::Result<(Decision, Decision)> {
        let assembled = assemble(&self.principals, &self.resources, req, Value::Array(vec![])).await?;
        Ok((evaluate(before, req, &assembled)?, evaluate(after, req, &assembled)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awzy_model::EntityRef;
    use serde_json::json;

    fn request(principal: &str) -> Check {
        Check {
            principal: EntityRef::new("Human", principal),
            action: "read".into(),
            resource: EntityRef::new("Resource", "board-deck"),
            context: Default::default(),
            on_behalf_of: None,
            human_present: Some(true),
            always_enforced: false,
            work_item_id: None,
        }
    }

    fn with_humans() -> Replayer {
        let principals = Arc::new(Principals::awaiting_sync());
        principals.load(
            1,
            vec![
                json!({ "uid": { "type": "Human", "id": "alice" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "security" }] }),
                json!({ "uid": { "type": "Human", "id": "bob" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "engineering" }] }),
            ],
        );
        Replayer::new(principals, Arc::new(Resources::none()))
    }

    #[tokio::test]
    async fn the_same_entities_decide_both_bundles() {
        let before = Pdp::with_defaults().unwrap();
        let after = Pdp::new(
            crate::DEFAULT_SCHEMA,
            &format!("{}\n@id(\"no-security-reads\")\nforbid (principal in Group::\"security\", action == Action::\"read\", resource);", crate::DEFAULT_POLICIES),
        )
        .unwrap();
        let replay = with_humans();
        let (b, a) = replay.compare(&before, &after, &request("alice")).await.unwrap();
        assert_eq!((b.verdict.as_str(), a.verdict.as_str()), ("allow", "deny"));
        assert_eq!(a.policies, ["no-security-reads"]);
        let (b, a) = replay.compare(&before, &after, &request("bob")).await.unwrap();
        assert_eq!(b.verdict, a.verdict, "bob isn't in security, so the draft doesn't touch him");
    }

    #[tokio::test]
    async fn a_policy_can_let_the_person_approved_for_s_manager_approve() {
        let principals = Arc::new(Principals::awaiting_sync());
        let human = |id: &str| json!({ "__entity": { "type": "Human", "id": id } });
        principals.load(
            1,
            vec![
                json!({ "uid": { "type": "Human", "id": "carol" }, "attrs": { "active": true }, "parents": [] }),
                json!({ "uid": { "type": "Human", "id": "bob" }, "attrs": { "active": true }, "parents": [] }),
                json!({ "uid": { "type": "Human", "id": "dave" }, "attrs": { "active": true, "manager": human("carol") }, "parents": [] }),
            ],
        );
        let replay = Replayer::new(principals, Arc::new(Resources::none()));
        let managers_approve = "@id(\"managers-approve\")\npermit (principal is Human, action == Action::\"approve\", resource)\n\
            when { context has approving_for && context.approving_for has manager && context.approving_for.manager == principal };";
        let pdp = Pdp::new(crate::DEFAULT_SCHEMA, &format!("{}\n{managers_approve}", crate::DEFAULT_POLICIES)).unwrap();
        let approve = |approver: &str| {
            let mut req = request(approver);
            req.action = "approve".into();
            req.context.insert("approving_for".into(), human("dave"));
            req
        };
        let (d, _) = replay.compare(&pdp, &pdp, &approve("carol")).await.unwrap();
        assert_eq!((d.verdict, d.policies), (Verdict::Allow, vec!["managers-approve".to_string()]));
        let (d, _) = replay.compare(&pdp, &pdp, &approve("bob")).await.unwrap();
        assert_eq!(d.verdict, Verdict::Deny, "not dave's manager");
    }

    #[test]
    fn a_named_work_item_alone_decides_who_is_assigned() {
        let work = HashMap::from([
            ("jira:SALES-1".to_string(), HashSet::from(["nora".to_string()])),
            ("jira:SALES-2".to_string(), HashSet::from(["sam".to_string()])),
        ]);
        let mut agent = request("sales-genie");
        agent.principal = EntityRef::new("ServiceAccount", "sales-genie");
        agent.on_behalf_of = Some(EntityRef::new("Human", "nora"));
        let named = |mut req: Check, item: &str| {
            req.work_item_id = Some(item.to_string());
            on_work_item(&req, &work)
        };
        assert_eq!(
            named(agent.clone(), "jira:SALES-1"),
            HashSet::from(["sales-genie".to_string(), "nora".to_string()]),
            "nora's ticket links it: the agent works within it"
        );
        assert!(named(agent.clone(), "jira:SALES-2").is_empty(), "a ticket she isn't on");
        assert!(named(agent, "jira:OTHER-9").is_empty(), "closed, unknown, or not linking it");
        assert_eq!(named(request("sam"), "jira:SALES-2"), HashSet::from(["sam".to_string()]));
    }

    #[tokio::test]
    async fn an_unregistered_service_account_is_denied_under_both() {
        let mut req = request("alice");
        req.principal = EntityRef::new("ServiceAccount", "nobody-registered-me");
        let pdp = Pdp::with_defaults().unwrap();
        let (b, a) = with_humans().compare(&pdp, &pdp, &req).await.unwrap();
        assert_eq!((b.verdict, a.verdict), (Verdict::Deny, Verdict::Deny));
        assert_eq!(b.policies, [principals::UNREGISTERED]);
    }
}

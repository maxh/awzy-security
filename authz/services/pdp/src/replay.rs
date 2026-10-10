//! Deciding a request the way a serving PDP would, outside one: for the policy dry-run
//! (ADR 0049), which replays recent requests under the active bundle and a draft.
//!
//! [`assemble`] is the one place that turns a request into Cedar entities. Live checks
//! (`AppState::check`) and replays both go through it, so a replay sees what a PDP would.

use std::{collections::HashSet, sync::Arc};

use awzy_model::{AccessRequest, Decision, DecisionId, EnforcementMode, Verdict};
use bytes::Bytes;
use http_body_util::Empty;
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
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
    },
}

/// The request's inline entities plus its principals from the snapshot and its resource from
/// awzy-context. Fails only when awzy-context can't be reached for the resource.
pub(crate) async fn assemble(
    principals: &Principals,
    resources: &Resources,
    req: &AccessRequest,
    inline: Value,
) -> anyhow::Result<Assembled> {
    if principals.is_unregistered(&req.subject) {
        return Ok(Assembled::Unregistered);
    }
    let who: Vec<&awzy_model::EntityRef> = std::iter::once(&req.subject).chain(req.on_behalf_of.as_ref()).collect();
    let entities = principals.merge(inline, &who);
    let merged = resources.merge(entities, &req.resource).await?;
    Ok(Assembled::Ready { entities: merged.entities, assignees: merged.assignees })
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

fn evaluate(pdp: &Pdp, req: &AccessRequest, assembled: &Assembled) -> anyhow::Result<Decision> {
    match assembled {
        Assembled::Unregistered => Ok(unregistered()),
        Assembled::Ready { entities, assignees } => {
            pdp.decide_assigned(req, &pdp.entities(entities.clone())?, assignees.as_deref())
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
        let client: Client<HttpConnector, Empty<Bytes>> = Client::builder(TokioExecutor::new()).build_http();
        principals::sync_once(&principals, endpoint, &client, &awzy_service_auth::TokenSource::from_env()).await?;
        Ok(Self::new(principals, Arc::new(Resources::from_context(endpoint.to_string()))))
    }

    /// Without awzy-context, as a PDP without it decides: only the request's own entities.
    pub fn without_context() -> Self {
        Self::new(Arc::new(Principals::none()), Arc::new(Resources::none()))
    }

    /// The request decided under `before` and under `after`, on the same entities.
    pub async fn compare(
        &self,
        before: &Pdp,
        after: &Pdp,
        req: &AccessRequest,
    ) -> anyhow::Result<(Decision, Decision)> {
        let assembled = assemble(&self.principals, &self.resources, req, Value::Array(vec![])).await?;
        Ok((evaluate(before, req, &assembled)?, evaluate(after, req, &assembled)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awzy_model::EntityRef;
    use serde_json::json;

    fn request(subject: &str) -> AccessRequest {
        AccessRequest {
            subject: EntityRef::new("Human", subject),
            action: "read".into(),
            resource: EntityRef::new("Resource", "board-deck"),
            context: Default::default(),
            on_behalf_of: None,
            human_present: Some(true),
            always_enforced: false,
        }
    }

    fn with_people() -> Replayer {
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
        let replay = with_people();
        let (b, a) = replay.compare(&before, &after, &request("alice")).await.unwrap();
        assert_eq!((b.verdict.as_str(), a.verdict.as_str()), ("allow", "deny"));
        assert_eq!(a.policies, ["no-security-reads"]);
        let (b, a) = replay.compare(&before, &after, &request("bob")).await.unwrap();
        assert_eq!(b.verdict, a.verdict, "bob isn't in security, so the draft doesn't touch him");
    }

    #[tokio::test]
    async fn an_unregistered_service_account_is_denied_under_both() {
        let mut req = request("alice");
        req.subject = EntityRef::new("ServiceAccount", "nobody-registered-me");
        let pdp = Pdp::with_defaults().unwrap();
        let (b, a) = with_people().compare(&pdp, &pdp, &req).await.unwrap();
        assert_eq!((b.verdict, a.verdict), (Verdict::Deny, Verdict::Deny));
        assert_eq!(b.policies, [principals::UNREGISTERED]);
    }
}

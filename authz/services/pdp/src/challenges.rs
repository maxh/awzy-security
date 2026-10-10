//! Enforced challenges, through awzy-challenge.
//!
//! On an enforced challenge verdict the PDP asks awzy-challenge which of the required kinds this
//! exact request has already passed, and gets the challenge to answer for the rest. Only
//! challenge verdicts pay for that call. If awzy-challenge can't answer, the verdict stays a
//! challenge with no ID: the request is never allowed for want of an answer.

use awzy_service_auth::{grpc_channel, AuthChannel};
use std::time::Duration;

use awzy_authz_proto::{
    challenge::v1::{challenge_service_client::ChallengeServiceClient, ResolveRequest},
    challenges_from_wire, challenges_to_wire,
};
use awzy_model::{AccessRequest, ChallengeId, Challenges, DecisionId, EntityRef};
use serde_json::Value;
use tonic::transport::Channel;

/// The reason recorded when an approver rejected the request. Not a Cedar policy.
pub const CHALLENGE_REJECTED: &str = "challenge-rejected";

/// Generous: a challenge verdict waits on a person anyway, and the first call after a PDP starts
/// also opens the connection. The same limit as an awzy-context lookup (ADR 0044).
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(1);

/// What awzy-challenge said about one challenged request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub passed: Challenges,
    pub challenge_id: Option<ChallengeId>,
    pub rejected: bool,
}

pub struct ChallengeResolver {
    client: Option<ChallengeServiceClient<AuthChannel>>,
}

impl ChallengeResolver {
    /// Without awzy-challenge: challenge verdicts go out with no ID, as before it existed.
    pub fn none() -> Self {
        Self { client: None }
    }

    pub fn from_challenge(endpoint: &str) -> anyhow::Result<Self> {
        let channel = Channel::from_shared(endpoint.to_string())?.connect_lazy();
        Ok(Self { client: Some(ChallengeServiceClient::new(grpc_channel(channel, "awzy-challenge"))) })
    }

    pub fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    pub async fn resolve(
        &self,
        decision_id: DecisionId,
        req: &AccessRequest,
        required: Challenges,
        parties: &Parties,
    ) -> anyhow::Result<Resolution> {
        let Some(client) = &self.client else { anyhow::bail!("awzy-challenge isn't configured") };
        let res = tokio::time::timeout(
            RESOLVE_TIMEOUT,
            client.clone().resolve(ResolveRequest {
                decision_qid: decision_id.to_string(),
                subject: Some(req.subject.clone().into()),
                on_behalf_of: req.on_behalf_of.clone().map(Into::into),
                action: req.action.clone(),
                resource: Some(req.resource.clone().into()),
                kinds: challenges_to_wire(required),
                answerer_id: parties.answerer.clone().unwrap_or_default(),
                open: true,
                resource_owner_id: parties.resource_owner.clone().unwrap_or_default(),
            }),
        )
        .await
        .map_err(|_| anyhow::anyhow!("awzy-challenge didn't answer in time"))??
        .into_inner();
        let challenge_id = match res.challenge_qid.as_str() {
            "" => None,
            id => Some(id.parse()?),
        };
        Ok(Resolution { passed: challenges_from_wire(&res.passed), challenge_id, rejected: res.rejected })
    }
}

/// Who a challenge involves besides the requester: the person it goes to, and the resource's
/// owner, told when it asks for an approval (ADR 0073).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parties {
    pub answerer: Option<String>,
    pub resource_owner: Option<String>,
}

pub fn parties(req: &AccessRequest, entities: &Value) -> Parties {
    Parties { answerer: answerer(req, entities), resource_owner: resource_owner(req, entities) }
}

/// The person who owns the requested resource, from its entity's `owner` (ADR 0061).
pub fn resource_owner(req: &AccessRequest, entities: &Value) -> Option<String> {
    let resource = entities
        .as_array()?
        .iter()
        .find(|e| e["uid"]["type"] == req.resource.kind.as_str() && e["uid"]["id"] == req.resource.id.as_str())?;
    human_ref(&resource["attrs"]["owner"])
}

/// A reference to a person in Cedar entity JSON, written either way.
fn human_ref(v: &Value) -> Option<String> {
    let v = if v.get("__entity").is_some() { &v["__entity"] } else { v };
    let r: EntityRef = serde_json::from_value(v.clone()).ok()?;
    (r.kind == awzy_model::entity_type::HUMAN).then_some(r.id)
}

/// The person a challenge goes to: the human subject, the person a service account acts for,
/// or a service account's owner (from its entity). None when no one is known.
pub fn answerer(req: &AccessRequest, entities: &Value) -> Option<String> {
    if let Some(human) = &req.on_behalf_of {
        return Some(human.id.clone());
    }
    if req.subject.kind == awzy_model::entity_type::HUMAN {
        return Some(req.subject.id.clone());
    }
    let subject = entities
        .as_array()?
        .iter()
        .find(|e| e["uid"]["type"] == req.subject.kind.as_str() && e["uid"]["id"] == req.subject.id.as_str())?;
    human_ref(&subject["attrs"]["owner"])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn request(subject: EntityRef, on_behalf_of: Option<EntityRef>) -> AccessRequest {
        AccessRequest {
            subject,
            action: "read".into(),
            resource: EntityRef::new("Resource", "plan"),
            context: Default::default(),
            on_behalf_of,
            human_present: None,
            always_enforced: false,
        }
    }

    #[test]
    fn challenges_go_to_the_person_responsible() {
        let none = json!([]);
        let alice = EntityRef::new("Human", "alice");
        let agent = EntityRef::new("ServiceAccount", "sales-genie");
        assert_eq!(answerer(&request(alice.clone(), None), &none).as_deref(), Some("alice"));
        assert_eq!(answerer(&request(agent.clone(), Some(alice)), &none).as_deref(), Some("alice"));
        assert_eq!(answerer(&request(agent.clone(), None), &none), None, "an unknown account's owner");

        for owner in
            [json!({ "__entity": { "type": "Human", "id": "olga" } }), json!({ "type": "Human", "id": "olga" })]
        {
            let entities = json!([{ "uid": { "type": "ServiceAccount", "id": "sales-genie" },
                "attrs": { "owner": owner, "ai": true }, "parents": [] }]);
            assert_eq!(answerer(&request(agent.clone(), None), &entities).as_deref(), Some("olga"));
        }
    }

    #[test]
    fn an_approval_is_also_told_to_the_resources_owner() {
        let alice = EntityRef::new("Human", "alice");
        let entities = json!([{ "uid": { "type": "Resource", "id": "plan" },
            "attrs": { "sensitivity": "crown_jewels", "owner": { "__entity": { "type": "Human", "id": "priya" } } },
            "parents": [] }]);
        let p = parties(&request(alice.clone(), None), &entities);
        assert_eq!(p, Parties { answerer: Some("alice".into()), resource_owner: Some("priya".into()) });
        assert_eq!(resource_owner(&request(alice, None), &json!([])), None, "an unknown resource has no owner");
    }
}

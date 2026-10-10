//! Enforced challenges, through awzy-challenge.
//!
//! On an enforced challenge verdict the PDP asks awzy-challenge which of the required kinds this
//! exact request has already passed, and gets the challenge to answer for the rest. Only
//! challenge verdicts pay for that call. If awzy-challenge can't answer, the verdict stays a
//! challenge with no ID: the request is never allowed for want of an answer.

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

const RESOLVE_TIMEOUT: Duration = Duration::from_millis(250);

/// What awzy-challenge said about one challenged request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub passed: Challenges,
    pub challenge_id: Option<ChallengeId>,
    pub rejected: bool,
}

pub struct ChallengeResolver {
    client: Option<ChallengeServiceClient<Channel>>,
}

impl ChallengeResolver {
    /// Without awzy-challenge: challenge verdicts go out with no ID, as before it existed.
    pub fn none() -> Self {
        Self { client: None }
    }

    pub fn from_challenge(endpoint: &str) -> anyhow::Result<Self> {
        let channel = Channel::from_shared(endpoint.to_string())?.connect_lazy();
        Ok(Self { client: Some(ChallengeServiceClient::new(channel)) })
    }

    pub fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    pub async fn resolve(
        &self,
        decision_id: DecisionId,
        req: &AccessRequest,
        required: Challenges,
        answerer: Option<&str>,
    ) -> anyhow::Result<Resolution> {
        let Some(client) = &self.client else { anyhow::bail!("awzy-challenge isn't configured") };
        let res = tokio::time::timeout(
            RESOLVE_TIMEOUT,
            client.clone().resolve(ResolveRequest {
                decision_id: decision_id.to_string(),
                subject: Some(req.subject.clone().into()),
                on_behalf_of: req.on_behalf_of.clone().map(Into::into),
                action: req.action.clone(),
                resource: Some(req.resource.clone().into()),
                kinds: challenges_to_wire(required),
                answerer_id: answerer.unwrap_or_default().to_string(),
                open: true,
            }),
        )
        .await
        .map_err(|_| anyhow::anyhow!("awzy-challenge didn't answer in time"))??
        .into_inner();
        let challenge_id = match res.challenge_id.as_str() {
            "" => None,
            id => Some(id.parse()?),
        };
        Ok(Resolution { passed: challenges_from_wire(&res.passed), challenge_id, rejected: res.rejected })
    }
}

/// The person a challenge goes to: the human subject, the person a service account acts for,
/// or a service account's owner (from its entity). None when no one is known.
pub fn answerer(req: &AccessRequest, entities: &Value) -> Option<String> {
    if let Some(person) = &req.on_behalf_of {
        return Some(person.id.clone());
    }
    if req.subject.kind == awzy_model::entity_type::HUMAN {
        return Some(req.subject.id.clone());
    }
    let subject = entities
        .as_array()?
        .iter()
        .find(|e| e["uid"]["type"] == req.subject.kind.as_str() && e["uid"]["id"] == req.subject.id.as_str())?;
    let owner = &subject["attrs"]["owner"];
    // Cedar entity JSON writes a reference either way.
    let owner = if owner.get("__entity").is_some() { &owner["__entity"] } else { owner };
    let owner: EntityRef = serde_json::from_value(owner.clone()).ok()?;
    (owner.kind == awzy_model::entity_type::HUMAN).then_some(owner.id)
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
}

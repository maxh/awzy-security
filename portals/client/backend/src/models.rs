//! The model routing (ADR 0114): which models Awzy Bot and investigations use, and how each
//! feature's conversations split between them. Kept and signed by awzy-policy; this portal and
//! awzy-reasoner follow it. A routing can only name the connections this install was set up with.

use async_graphql::{Context, InputObject, Json, Object, Result, SimpleObject};
use awzy_authz_proto::policy::v1::{GetModelRoutingRequest, ModelRouting as WireRouting, SetModelRoutingRequest};
use awzy_llm::{Connection, Routing};
use awzy_model::ChallengeId;
use awzy_portal::{Actor, Qid, UtcTimestamp};
use chrono::DateTime;
use serde_json::Value;

use crate::{
    access::{require, PortalAction::*, PortalArea},
    enforcement::switch_challenge,
    policy_client, Services,
};

/// A model service this install can reach.
#[derive(SimpleObject)]
pub struct ModelConnection {
    /// What a model's `connection` names, e.g. `cloud`.
    pub name: String,
    /// bedrock, vertex, openai, or anthropic.
    pub kind: String,
}

pub struct ModelRoutingSetting {
    wire: WireRouting,
    connections: Vec<ModelConnection>,
}

#[Object(name = "ModelRouting")]
impl ModelRoutingSetting {
    /// 0 when never set: each service uses the routing its deploy configuration gave it.
    async fn version(&self) -> i64 {
        self.wire.version
    }

    /// The catalog and the routes, as awzy_llm's Routing: `{ models: [{ name, connection, spec }],
    /// routes: { awzy_bot: { targets: [{ model, weight }], fallback: [] }, … } }`.
    async fn routing(&self) -> Json<Value> {
        Json(serde_json::from_str(&self.wire.routing_json).unwrap_or(Value::Null))
    }

    /// The services this install can reach, which a routing may name.
    async fn connections(&self) -> &[ModelConnection] {
        &self.connections
    }

    async fn changed_by(&self) -> Option<Actor> {
        awzy_authz_proto::actor_from_wire(self.wire.changed_by_actor.clone(), "")
            .and_then(|a| Actor::from_text(&a.to_string()))
    }

    async fn changed_at(&self) -> Option<UtcTimestamp> {
        self.wire
            .changed_at
            .as_ref()
            .and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32))
            .map(UtcTimestamp)
    }
}

/// The connections this portal's router has; none when no feature here uses models.
fn connections(ctx: &Context<'_>) -> Result<awzy_llm::Connections> {
    let services = ctx.data::<Services>()?;
    Ok(services.bot.as_ref().and_then(|b| b.router()).map(|r| r.connections().clone()).unwrap_or_default())
}

fn described(connections: &awzy_llm::Connections) -> Vec<ModelConnection> {
    connections
        .iter()
        .map(|(name, c)| ModelConnection {
            name: name.clone(),
            kind: match c {
                Connection::Bedrock { .. } => "bedrock",
                Connection::Vertex { .. } => "vertex",
                Connection::Openai { .. } => "openai",
                Connection::Anthropic { .. } => "anthropic",
            }
            .into(),
        })
        .collect()
}

#[derive(Default)]
pub struct ModelsQuery;

#[Object]
impl ModelsQuery {
    /// Which models Awzy Bot and investigations use, and the services this install can reach.
    async fn model_routing(&self, ctx: &Context<'_>) -> Result<ModelRoutingSetting> {
        require(ctx, View, PortalArea::Policies).await?;
        let res = policy_client(ctx)?.get_model_routing(GetModelRoutingRequest { known_version: -1 }).await?;
        let wire = res.into_inner().routing.ok_or("awzy-policy returned no model routing")?;
        Ok(ModelRoutingSetting { wire, connections: described(&connections(ctx)?) })
    }
}

#[derive(InputObject)]
pub struct SetModelRoutingInput {
    /// The whole catalog and every route, as `ModelRouting.routing` reads.
    pub routing: Json<Value>,
}

#[derive(SimpleObject)]
pub struct SetModelRoutingPayload {
    /// The new setting; null when a challenge must be passed first.
    pub model_routing: Option<ModelRoutingSetting>,
    /// The challenge to pass, then save again.
    pub challenge_qid: Option<Qid<ChallengeId>>,
}

#[derive(Default)]
pub struct ModelsMutation;

#[Object]
impl ModelsMutation {
    /// Replaces the model routing. It's checked against this install's connections first, so it
    /// can't send prompts anywhere the install wasn't set up to reach. Like switching enforcement,
    /// policy can ask for a security key first: then nothing changes, and the payload names the
    /// challenge to pass.
    async fn set_model_routing(&self, ctx: &Context<'_>, input: SetModelRoutingInput) -> Result<SetModelRoutingPayload> {
        let viewer = require(ctx, Administer, PortalArea::Policies).await?;
        let routing: Routing = serde_json::from_value(input.routing.0)
            .map_err(|e| awzy_portal::ErrorCode::InvalidInput.error(format!("not a model routing: {e}")))?;
        let connections = connections(ctx)?;
        // Before any challenge, so a mistake is reported without a touch of the key.
        routing.validate(&connections).map_err(|e| awzy_portal::ErrorCode::InvalidInput.error(e.to_string()))?;
        if let Some(challenge_qid) = switch_challenge(ctx, &viewer).await? {
            return Ok(SetModelRoutingPayload { model_routing: None, challenge_qid: Some(challenge_qid) });
        }
        let req = SetModelRoutingRequest {
            routing_json: serde_json::to_string(&routing)?,
            changed_by_actor: Some(viewer.actor().into()),
        };
        let wire = policy_client(ctx)?.set_model_routing(req).await?.into_inner().routing;
        Ok(SetModelRoutingPayload {
            model_routing: Some(ModelRoutingSetting {
                wire: wire.ok_or("awzy-policy returned no model routing")?,
                connections: described(&connections),
            }),
            challenge_qid: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use awzy_llm::{Connections, ModelRouter};
    use serde_json::json;

    use crate::{
        awzy_bot::BotModel,
        policies::tests::{serve_fake_policy, FakePolicy},
        testing, Services,
    };

    #[tokio::test]
    async fn an_admin_sets_a_routing_only_over_the_installs_connections() {
        let connections: Connections = serde_json::from_value(json!({
            "cloud": { "kind": "vertex", "project": "acme" },
            "vllm": { "kind": "openai", "url": "http://vllm:8000/v1" },
        }))
        .unwrap();
        let router = ModelRouter::new(connections, awzy_llm::Routing::default()).await.unwrap();
        // The base policies ask for a security key first; this test is about the routing.
        let ask = "@id(\"enforcement-switches-need-a-key\")\n@challenge(\"verification\")\n\
                   forbid (principal, action == Action::\"switch_enforcement\", resource);\n";
        let schema = crate::schema_with(Services {
            policy: Some(serve_fake_policy(FakePolicy::default()).await),
            pdp: Some(testing::pdp_with(&awzy_pdp::DEFAULT_POLICIES.replace(ask, "")).await),
            bot: Some(BotModel::Routed(Arc::new(router))),
            ..Default::default()
        });
        let alice = testing::human("alice", "security");
        let set = |routing: serde_json::Value| {
            format!(
                "mutation {{ setModelRouting(input: {{ routing: {} }}) {{ modelRouting {{ version }} }} }}",
                async_graphql::Value::from_json(routing).unwrap()
            )
        };

        let elsewhere = json!({
            "models": [{ "name": "gpt", "connection": "openai-public", "spec": { "api": "openai_chat", "model": "gpt" } }],
            "routes": { "awzy_bot": { "targets": [{ "model": "gpt" }] } },
        });
        let err = testing::run_err(&schema, &alice, &set(elsewhere)).await;
        assert!(err.contains("doesn't have"), "{err}");

        let routing = json!({
            "models": [
                { "name": "gemini", "connection": "cloud", "spec": { "api": "gemini", "model": "gemini-2.5-pro" } },
                { "name": "llama", "connection": "vllm", "spec": { "api": "openai_chat", "model": "llama" } },
            ],
            "routes": { "awzy_bot": { "targets": [{ "model": "gemini", "weight": 9 }, { "model": "llama", "weight": 1 }] } },
        });
        let saved = testing::run(&schema, &alice, &set(routing.clone())).await;
        assert_eq!(saved["setModelRouting"]["modelRouting"]["version"], 1);

        let read = testing::run(&schema, &alice, "{ modelRouting { version routing changedBy { id } connections { name kind } } }").await;
        assert_eq!(read["modelRouting"]["routing"]["routes"]["awzy_bot"]["targets"][0]["weight"], 9);
        assert_eq!(read["modelRouting"]["changedBy"]["id"], "alice");
        assert_eq!(read["modelRouting"]["connections"], json!([{ "name": "cloud", "kind": "vertex" }, { "name": "vllm", "kind": "openai" }]));
    }
}

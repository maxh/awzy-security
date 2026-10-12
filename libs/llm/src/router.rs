//! Which model each conversation goes to (ADR 0114): a catalog of models on the install's
//! connections, and a route per feature that splits conversations between them by weight, with
//! models to fall back on when one is busy, down, or not set up. The routing is a runtime setting
//! (awzy-policy keeps it, the portal edits it) that a router takes in without restarting.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, RwLock,
    },
};

use serde::{Deserialize, Serialize};

use crate::{
    connections::{self, Endpoint},
    Connections, LlmError, ModelProvider, ModelRequest, ModelResponse, ModelSpec, Models,
};

/// A model the install may use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEntry {
    /// What routes and records call it, e.g. `claude-opus` or `gemini-pro`.
    pub name: String,
    /// The connection it's reached through.
    pub connection: String,
    /// The model, in the terms of the API it speaks.
    pub spec: ModelSpec,
}

/// A share of a feature's conversations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteTarget {
    pub model: String,
    /// Relative to the route's other targets; 0 takes none.
    #[serde(default = "one")]
    pub weight: u32,
}

fn one() -> u32 {
    1
}

/// Where one feature's conversations go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    pub targets: Vec<RouteTarget>,
    /// Tried in order when the chosen model is busy, down, or not set up.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback: Vec<String>,
}

/// The catalog and the routes: the setting a router follows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Routing {
    #[serde(default)]
    pub models: Vec<ModelEntry>,
    /// By feature: `awzy_bot`, `investigations`.
    #[serde(default)]
    pub routes: BTreeMap<String, Route>,
}

impl Routing {
    /// Whether every name resolves: models to known connections, routes to models in the catalog.
    pub fn validate(&self, connections: &Connections) -> Result<(), LlmError> {
        let mut names = BTreeSet::new();
        for m in &self.models {
            if m.name.trim().is_empty() {
                return Err(LlmError::NotConfigured("a model has no name".into()));
            }
            if !names.insert(m.name.as_str()) {
                return Err(LlmError::NotConfigured(format!("two models are named {:?}", m.name)));
            }
            let Some(connection) = connections.get(&m.connection) else {
                return Err(LlmError::NotConfigured(format!(
                    "model {:?} uses connection {:?}, which this install doesn't have (it has: {})",
                    m.name,
                    m.connection,
                    connections.keys().cloned().collect::<Vec<_>>().join(", ")
                )));
            };
            if !m.spec.offered_by(connection) {
                return Err(LlmError::NotConfigured(format!("model {:?}: {}", m.name, m.spec.mismatch(connection))));
            }
        }
        for (feature, route) in &self.routes {
            if route.targets.iter().all(|t| t.weight == 0) {
                return Err(LlmError::NotConfigured(format!(
                    "route {feature:?} sends conversations nowhere: no target has weight"
                )));
            }
            for name in route.targets.iter().map(|t| &t.model).chain(&route.fallback) {
                if !names.contains(name.as_str()) {
                    return Err(LlmError::NotConfigured(format!(
                        "route {feature:?} names model {name:?}, which isn't in the catalog"
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Sends each conversation to a model by the routing in force.
pub struct ModelRouter {
    connections: Connections,
    endpoints: HashMap<String, Arc<Endpoint>>,
    routing: RwLock<Arc<Routing>>,
    /// Each route's running weights, for smooth weighted round-robin.
    turns: Mutex<HashMap<String, Vec<i64>>>,
}

impl ModelRouter {
    /// Connects to each of `connections`, under `routing` until another is set.
    pub async fn new(connections: Connections, routing: Routing) -> Result<Self, LlmError> {
        routing.validate(&connections)?;
        let mut endpoints = HashMap::new();
        for (name, c) in &connections {
            endpoints.insert(name.clone(), Arc::new(Endpoint::connect(c).await?));
        }
        Ok(Self { connections, endpoints, routing: RwLock::new(Arc::new(routing)), turns: Mutex::default() })
    }

    /// AWZY_LLM_CONNECTIONS (what the install can reach), and AWZY_LLM_ROUTING, the routing to
    /// start with (JSON; none when unset) until awzy-policy's setting arrives.
    pub async fn from_env() -> Result<Self, LlmError> {
        let routing = match std::env::var("AWZY_LLM_ROUTING").ok().filter(|v| !v.trim().is_empty()) {
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| LlmError::NotConfigured(format!("AWZY_LLM_ROUTING isn't valid: {e}")))?,
            None => Routing::default(),
        };
        Self::new(connections::from_env()?, routing).await
    }

    /// The connections' names, for checking a routing before it's set.
    pub fn connections(&self) -> &Connections {
        &self.connections
    }

    pub fn routing(&self) -> Arc<Routing> {
        self.routing.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Takes `routing` for the next conversations; ones under way keep their model.
    pub fn set_routing(&self, routing: Routing) -> Result<(), LlmError> {
        routing.validate(&self.connections)?;
        *self.routing.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(routing);
        self.turns.lock().unwrap_or_else(|e| e.into_inner()).clear();
        Ok(())
    }

    /// Whether `feature` has a route, so the feature can say it's off rather than fail.
    pub fn routes(&self, feature: &str) -> bool {
        self.routing().routes.contains_key(feature)
    }

    /// A model for one conversation of `feature`: its route's next target by weight, then the
    /// route's fallbacks.
    pub fn conversation(&self, feature: &str) -> Result<Conversation, LlmError> {
        let routing = self.routing();
        let route = routing
            .routes
            .get(feature)
            .ok_or_else(|| LlmError::NotConfigured(format!("no model is routed for {feature}")))?;
        let first = &route.targets[self.next_target(feature, route)].model;
        let mut names = vec![first.clone()];
        names.extend(route.fallback.iter().filter(|n| *n != first).cloned());
        let chain = names
            .iter()
            .filter_map(|n| routing.models.iter().find(|m| &m.name == n))
            .filter_map(|m| Some((m.clone(), self.endpoints.get(&m.connection)?.clone())))
            .collect();
        Ok(Conversation { chain, at: AtomicUsize::new(0) })
    }

    /// Smooth weighted round-robin: over any run of conversations, each target gets its share,
    /// spread out rather than in bursts.
    fn next_target(&self, feature: &str, route: &Route) -> usize {
        let mut turns = self.turns.lock().unwrap_or_else(|e| e.into_inner());
        let current = turns.entry(feature.to_string()).or_default();
        if current.len() != route.targets.len() {
            *current = vec![0; route.targets.len()];
        }
        let total: i64 = route.targets.iter().map(|t| i64::from(t.weight)).sum();
        for (c, t) in current.iter_mut().zip(&route.targets) {
            *c += i64::from(t.weight);
        }
        let best = (0..current.len()).max_by_key(|&i| (current[i], std::cmp::Reverse(i))).unwrap_or(0);
        current[best] -= total;
        best
    }
}

impl Models for ModelRouter {
    type Conversation = Conversation;
    fn conversation(&self, feature: &str) -> Result<Conversation, LlmError> {
        ModelRouter::conversation(self, feature)
    }
}

impl Models for Arc<ModelRouter> {
    type Conversation = Conversation;
    fn conversation(&self, feature: &str) -> Result<Conversation, LlmError> {
        ModelRouter::conversation(self, feature)
    }
}

/// One conversation's model, and where it falls back to. Once it falls back it stays there, and
/// the earlier model's opaque blocks are left out of what the next one is sent.
pub struct Conversation {
    chain: Vec<(ModelEntry, Arc<Endpoint>)>,
    at: AtomicUsize,
}

impl Conversation {
    /// The catalog name of the model answering now.
    pub fn model(&self) -> &str {
        self.chain.get(self.at.load(Ordering::Relaxed)).map_or("", |(m, _)| &m.name)
    }
}

impl ModelProvider for Conversation {
    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, LlmError> {
        let mut last = LlmError::NotConfigured("the route's models aren't in the catalog".into());
        for i in self.at.load(Ordering::Relaxed)..self.chain.len() {
            let (entry, endpoint) = &self.chain[i];
            match endpoint.respond(entry, request).await {
                Ok(mut response) => {
                    self.at.store(i, Ordering::Relaxed);
                    response.model = entry.name.clone();
                    return Ok(response);
                }
                // An answer the model wrote but awzy couldn't read won't read better elsewhere.
                Err(e @ LlmError::Malformed(_)) => return Err(e),
                Err(e) => {
                    tracing::warn!(model = %entry.name, error = %e, "model unavailable; trying the route's next");
                    last = e;
                }
            }
        }
        Err(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Connection;
    use serde_json::json;

    fn connections() -> Connections {
        [
            ("aws".to_string(), Connection::Bedrock { region: "us-east-2".into() }),
            ("vllm".to_string(), Connection::Openai { url: "http://vllm:8000/v1".into(), api_key_env: None }),
        ]
        .into()
    }

    fn routing(weights: (u32, u32)) -> Routing {
        serde_json::from_value(json!({
            "models": [
                { "name": "claude", "connection": "aws",
                  "spec": { "api": "converse", "model_id": "us.anthropic.claude-opus-5-5", "cache_points": true } },
                { "name": "llama", "connection": "vllm",
                  "spec": { "api": "openai_chat", "model": "meta-llama/Llama-4", "temperature": 0.2 } },
            ],
            "routes": {
                "awzy_bot": { "targets": [{ "model": "claude", "weight": weights.0 }, { "model": "llama", "weight": weights.1 }],
                              "fallback": ["llama"] },
            },
        }))
        .unwrap()
    }

    #[test]
    fn a_routing_only_names_connections_the_install_has_and_models_in_its_catalog() {
        assert!(routing((9, 1)).validate(&connections()).is_ok());
        let mut r = routing((9, 1));
        r.models[1].connection = "openai-public".into();
        assert!(r.validate(&connections()).unwrap_err().to_string().contains("doesn't have"));
        let mut r = routing((9, 1));
        r.models[1].connection = "aws".into();
        assert!(r.validate(&connections()).unwrap_err().to_string().contains("doesn't offer the openai_chat API"));
        let mut r = routing((9, 1));
        r.routes.get_mut("awzy_bot").unwrap().fallback.push("gpt".into());
        assert!(r.validate(&connections()).unwrap_err().to_string().contains("isn't in the catalog"));
        assert!(routing((0, 0)).validate(&connections()).unwrap_err().to_string().contains("nowhere"));
    }

    #[tokio::test]
    async fn conversations_split_by_weight_and_a_new_routing_takes_over() {
        let router = ModelRouter::new(connections(), routing((3, 1))).await.unwrap();
        let picks: Vec<String> = (0..8).map(|_| router.conversation("awzy_bot").unwrap().model().to_string()).collect();
        assert_eq!(picks.iter().filter(|m| *m == "claude").count(), 6, "{picks:?}");
        assert_eq!(picks.iter().filter(|m| *m == "llama").count(), 2, "{picks:?}");
        assert_ne!(picks[..4].iter().filter(|m| *m == "llama").count(), 0, "spread out, not in a burst: {picks:?}");

        router.set_routing(routing((0, 1))).unwrap();
        assert!((0..4).all(|_| router.conversation("awzy_bot").unwrap().model() == "llama"));
        assert!(router.set_routing(Routing { models: vec![], ..routing((1, 1)) }).is_err(), "and a bad one is refused");
        assert!(matches!(router.conversation("investigations"), Err(LlmError::NotConfigured(_))));
    }
}

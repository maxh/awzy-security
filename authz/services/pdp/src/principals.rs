//! People and groups from awzy-context (ADR 0031).
//!
//! The PDP keeps awzy-context's snapshot in memory and polls for a newer version. For each check
//! it takes the principals' own entities and their group ancestry from the snapshot, replacing
//! whatever the enforcement point sent for them: the IdP's current memberships win over a token
//! minted before a change, and a deactivated person carries no groups. Principals the snapshot
//! doesn't know (service accounts, people not provisioned) keep what the enforcement point sent.

use awzy_service_auth::TokenSource;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use awzy_model::EntityRef;
use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use serde_json::Value;

/// The reason recorded for a service account that isn't registered in awzy-context. Not a
/// Cedar policy: the PDP denies before evaluating any.
pub const UNREGISTERED: &str = "unregistered-service-account";

/// `Type::id`, the key entities are looked up by.
fn key(kind: &str, id: &str) -> String {
    format!("{kind}::{id}")
}

fn uid_key(uid: &Value) -> Option<String> {
    Some(key(uid["type"].as_str()?, uid["id"].as_str()?))
}

#[derive(Default)]
struct Snapshot {
    version: i64,
    entities: HashMap<String, Value>,
}

pub struct Principals {
    snapshot: ArcSwap<Snapshot>,
    /// False when the PDP runs without awzy-context: then it's always ready and adds nothing.
    configured: bool,
    loaded: AtomicBool,
}

impl Principals {
    /// No awzy-context: the enforcement points' entities are used as they are.
    pub fn none() -> Self {
        Self { snapshot: ArcSwap::from_pointee(Snapshot::default()), configured: false, loaded: AtomicBool::new(true) }
    }

    /// Waits for the first snapshot from awzy-context before the PDP is ready.
    pub fn awaiting_sync() -> Self {
        Self { snapshot: ArcSwap::from_pointee(Snapshot::default()), configured: true, loaded: AtomicBool::new(false) }
    }

    pub fn is_ready(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
    }

    /// A service account that awzy-context doesn't know, when the PDP uses awzy-context.
    pub fn is_unregistered(&self, subject: &EntityRef) -> bool {
        self.configured
            && subject.kind == awzy_model::entity_type::SERVICE_ACCOUNT
            && !self.snapshot.load().entities.contains_key(&key(&subject.kind, &subject.id))
    }

    pub fn version(&self) -> i64 {
        self.snapshot.load().version
    }

    /// Replaces the snapshot with these Cedar entities.
    pub fn load(&self, version: i64, entities: Vec<Value>) {
        let entities = entities.into_iter().filter_map(|e| Some((uid_key(&e["uid"])?, e))).collect();
        self.snapshot.store(Arc::new(Snapshot { version, entities }));
        self.loaded.store(true, Ordering::Release);
    }

    /// The request's entities with each known principal, and its groups up the hierarchy,
    /// taken from the snapshot instead.
    pub fn merge(&self, request: Value, principals: &[&EntityRef]) -> Value {
        if !self.configured {
            return request;
        }
        let snapshot = self.snapshot.load();
        let mut seen = HashSet::new();
        let mut queue: VecDeque<String> = principals.iter().map(|p| key(&p.kind, &p.id)).collect();
        let mut from_snapshot = Vec::new();
        while let Some(k) = queue.pop_front() {
            if !seen.insert(k.clone()) {
                continue;
            }
            let Some(entity) = snapshot.entities.get(&k) else { continue };
            for parent in entity["parents"].as_array().into_iter().flatten() {
                queue.extend(uid_key(parent));
            }
            from_snapshot.push(entity.clone());
        }
        if from_snapshot.is_empty() {
            return request;
        }
        let replaced: HashSet<String> = from_snapshot.iter().filter_map(|e| uid_key(&e["uid"])).collect();
        let mut entities: Vec<Value> = match request {
            Value::Array(items) => {
                items.into_iter().filter(|e| uid_key(&e["uid"]).is_none_or(|k| !replaced.contains(&k))).collect()
            }
            other => return other,
        };
        entities.extend(from_snapshot);
        Value::Array(entities)
    }
}

/// A GET to awzy-context, with this process's service token for it (ADR 0054).
pub(crate) fn context_get(url: &str, tokens: &TokenSource) -> anyhow::Result<axum::http::Request<Empty<Bytes>>> {
    let mut req = axum::http::Request::get(url);
    if let Some(authorization) = tokens.authorization("awzy-context")? {
        req = req.header(axum::http::header::AUTHORIZATION, authorization);
    }
    Ok(req.body(Empty::new())?)
}

/// Polls awzy-context's `/v1/principals` every `every`, forever. Failures keep the snapshot in
/// hand and are retried.
pub async fn sync_principals(principals: Arc<Principals>, endpoint: String, every: Duration) {
    let client: Client<HttpConnector, Empty<Bytes>> = Client::builder(TokioExecutor::new()).build_http();
    let tokens = TokenSource::from_env();
    loop {
        if let Err(error) = sync_once(&principals, &endpoint, &client, &tokens).await {
            tracing::warn!(%error, %endpoint, "principals sync failed; keeping the current snapshot");
        }
        tokio::time::sleep(every).await;
    }
}

/// One poll. Returns true when a new snapshot was loaded.
pub async fn sync_once(
    principals: &Principals,
    endpoint: &str,
    client: &Client<HttpConnector, Empty<Bytes>>,
    tokens: &TokenSource,
) -> anyhow::Result<bool> {
    let known = principals.is_ready().then(|| principals.version());
    let url = match known {
        Some(v) => format!("{endpoint}/v1/principals?known={v}"),
        None => format!("{endpoint}/v1/principals"),
    };
    let res = tokio::time::timeout(Duration::from_secs(10), client.request(context_get(&url, tokens)?)).await??;
    anyhow::ensure!(res.status().is_success(), "{url} returned {}", res.status());
    let body: Value = serde_json::from_slice(&res.into_body().collect().await?.to_bytes())?;
    let version = body["version"].as_i64().ok_or_else(|| anyhow::anyhow!("no version in the principals snapshot"))?;
    if body["unchanged"] == true {
        return Ok(false);
    }
    let entities = body["entities"]
        .as_array()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no entities in the principals snapshot"))?;
    let count = entities.len();
    principals.load(version, entities);
    tracing::info!(version, entities = count, "loaded principals from awzy-context");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn principals_and_their_groups_come_from_the_snapshot() {
        let p = Principals::awaiting_sync();
        assert!(!p.is_ready());
        p.load(
            3,
            vec![
                json!({ "uid": { "type": "Human", "id": "bob" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "appsec" }] }),
                json!({ "uid": { "type": "Group", "id": "appsec" }, "attrs": {}, "parents": [{ "type": "Group", "id": "security" }] }),
                json!({ "uid": { "type": "Group", "id": "security" }, "attrs": {}, "parents": [] }),
                json!({ "uid": { "type": "Human", "id": "carol" }, "attrs": { "active": true }, "parents": [] }),
            ],
        );
        assert!(p.is_ready());
        let request = json!([
            { "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] },
            { "uid": { "type": "Resource", "id": "doc" }, "attrs": { "sensitivity": "internal" }, "parents": [] },
        ]);
        let merged = p.merge(request, &[&EntityRef::new("Human", "bob")]);
        let ids: Vec<_> =
            merged.as_array().unwrap().iter().map(|e| e["uid"]["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(ids, ["doc", "bob", "appsec", "security"], "the token's groups are replaced; carol isn't needed");
        let bob = merged.as_array().unwrap().iter().find(|e| e["uid"]["id"] == "bob").unwrap();
        assert_eq!(bob["parents"], json!([{ "type": "Group", "id": "appsec" }]));

        let unknown = json!([{ "uid": { "type": "ServiceAccount", "id": "sales-genie" }, "attrs": {}, "parents": [] }]);
        assert_eq!(
            p.merge(unknown.clone(), &[&EntityRef::new("ServiceAccount", "sales-genie")]),
            unknown,
            "not in the snapshot: unchanged"
        );
        assert_eq!(Principals::none().merge(unknown.clone(), &[&EntityRef::new("Human", "bob")]), unknown);
    }
}

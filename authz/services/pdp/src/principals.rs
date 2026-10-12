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
use http_body_util::{BodyExt, Full};
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
    /// Risk scores from awzy-reasoner (ADR 0056), keyed `Type::id`.
    risk: HashMap<String, i64>,
    /// People's automation scores from awzy-reasoner (ADR 0085), keyed `Type::id`.
    automation: HashMap<String, i64>,
}

pub struct Principals {
    snapshot: ArcSwap<Snapshot>,
    /// Apps trusted to sign tokens for their own agents (ADR 0110), which enforcement points ask
    /// the PDP for. Not used in checks.
    issuers: ArcSwap<Vec<awzy_model::TrustedIssuer>>,
    /// False when the PDP runs without awzy-context: then it's always ready and adds nothing.
    configured: bool,
    loaded: AtomicBool,
}

impl Principals {
    /// No awzy-context: the enforcement points' entities are used as they are.
    pub fn none() -> Self {
        Self {
            snapshot: ArcSwap::from_pointee(Snapshot::default()),
            issuers: ArcSwap::default(),
            configured: false,
            loaded: AtomicBool::new(true),
        }
    }

    /// Waits for the first snapshot from awzy-context before the PDP is ready.
    pub fn awaiting_sync() -> Self {
        Self {
            snapshot: ArcSwap::from_pointee(Snapshot::default()),
            issuers: ArcSwap::default(),
            configured: true,
            loaded: AtomicBool::new(false),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
    }

    /// A service account that awzy-context doesn't know, when the PDP uses awzy-context.
    pub fn is_unregistered(&self, principal: &EntityRef) -> bool {
        self.configured
            && principal.kind == awzy_model::entity_type::SERVICE_ACCOUNT
            && !self.snapshot.load().entities.contains_key(&key(&principal.kind, &principal.id))
    }

    /// The apps trusted to sign tokens for their own agents, as of the last snapshot.
    pub fn trusted_issuers(&self) -> Arc<Vec<awzy_model::TrustedIssuer>> {
        self.issuers.load_full()
    }

    pub fn set_trusted_issuers(&self, issuers: Vec<awzy_model::TrustedIssuer>) {
        self.issuers.store(Arc::new(issuers));
    }

    pub fn version(&self) -> i64 {
        self.snapshot.load().version
    }

    /// Replaces the snapshot with these Cedar entities.
    pub fn load(&self, version: i64, entities: Vec<Value>) {
        self.load_with_risk(version, entities, HashMap::new());
    }

    /// The scores of `who` (the principal and the person it acts for) that awzy has, keyed
    /// `Type::id`: what each check sees as `context.risk_score` (ADR 0056).
    pub fn risk_of(&self, who: &[&EntityRef]) -> HashMap<String, i64> {
        Self::scores_of(&self.snapshot.load().risk, who)
    }

    /// The automation scores of `who` that awzy has, keyed `Type::id`: what each check sees as
    /// `context.automation_score` (ADR 0085).
    pub fn automation_of(&self, who: &[&EntityRef]) -> HashMap<String, i64> {
        Self::scores_of(&self.snapshot.load().automation, who)
    }

    fn scores_of(scores: &HashMap<String, i64>, who: &[&EntityRef]) -> HashMap<String, i64> {
        who.iter()
            .filter_map(|p| {
                let k = key(&p.kind, &p.id);
                scores.get(&k).map(|score| (k, *score))
            })
            .collect()
    }

    /// Replaces the snapshot with these Cedar entities and risk scores.
    pub fn load_with_risk(&self, version: i64, entities: Vec<Value>, risk: HashMap<String, i64>) {
        self.load_with_scores(version, entities, risk, HashMap::new());
    }

    /// Replaces the snapshot with these Cedar entities, risk scores, and automation scores.
    pub fn load_with_scores(
        &self,
        version: i64,
        entities: Vec<Value>,
        risk: HashMap<String, i64>,
        automation: HashMap<String, i64>,
    ) {
        let entities = entities.into_iter().filter_map(|e| Some((uid_key(&e["uid"])?, e))).collect();
        self.snapshot.store(Arc::new(Snapshot { version, entities, risk, automation }));
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

/// A GET to awzy-context, with this process's service token for it (ADR 0055).
pub(crate) fn context_get(url: &str, tokens: &TokenSource) -> anyhow::Result<axum::http::Request<Full<Bytes>>> {
    let mut req = axum::http::Request::get(url);
    if let Some(authorization) = tokens.authorization("awzy-context")? {
        req = req.header(axum::http::header::AUTHORIZATION, authorization);
    }
    Ok(req.body(Full::default())?)
}

/// Follows awzy-context's `/v1/principals` forever: each request waits there for the snapshot
/// to move (ADR 0077), so a change reaches this PDP as it's made. Failures keep the snapshot in
/// hand and are retried after `every`.
pub async fn sync_principals(principals: Arc<Principals>, endpoint: String, every: Duration) {
    let client: awzy_http::Client = awzy_http::client();
    let tokens = TokenSource::from_env();
    loop {
        // Nothing to wait against until the first snapshot.
        let wait = principals.is_ready().then_some(WAIT);
        match sync_waiting(&principals, &endpoint, &client, &tokens, wait).await {
            Ok(_) => crate::metrics::PRINCIPALS_SYNC.succeeded(),
            Err(error) => {
                crate::metrics::PRINCIPALS_SYNC.failed();
                tracing::warn!(%error, %endpoint, "principals sync failed; keeping the current snapshot");
                tokio::time::sleep(every).await;
                continue;
            }
        }
        if wait.is_none() {
            tokio::time::sleep(every).await;
        }
    }
}

/// How long awzy-context holds a request when nothing has changed.
const WAIT: Duration = Duration::from_secs(25);

/// One request, answered at once. Returns true when a new snapshot was loaded.
pub async fn sync_once(
    principals: &Principals,
    endpoint: &str,
    client: &awzy_http::Client,
    tokens: &TokenSource,
) -> anyhow::Result<bool> {
    sync_waiting(principals, endpoint, client, tokens, None).await
}

/// One request, which awzy-context holds up to `wait` for the snapshot to move.
pub async fn sync_waiting(
    principals: &Principals,
    endpoint: &str,
    client: &awzy_http::Client,
    tokens: &TokenSource,
    wait: Option<Duration>,
) -> anyhow::Result<bool> {
    let known = principals.is_ready().then(|| principals.version());
    let url = match (known, wait) {
        (Some(v), Some(w)) => format!("{endpoint}/v1/principals?known={v}&wait={}", w.as_secs()),
        (Some(v), None) => format!("{endpoint}/v1/principals?known={v}"),
        (None, _) => format!("{endpoint}/v1/principals"),
    };
    let limit = Duration::from_secs(10) + wait.unwrap_or_default();
    let res = tokio::time::timeout(limit, client.request(context_get(&url, tokens)?)).await??;
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
    // Absent from an awzy-context older than ADR 0056: no scores.
    let scores = |field: &str| -> HashMap<String, i64> {
        body[field].as_object().into_iter().flatten().filter_map(|(k, v)| Some((k.clone(), v.as_i64()?))).collect()
    };
    // Either is absent from an awzy-context older than it: no scores.
    principals.load_with_scores(version, entities, scores("risk"), scores("automation"));
    // Absent from an awzy-context older than ADR 0110: none.
    let issuers = match body.get("token_issuers") {
        Some(list) => serde_json::from_value(list.clone())?,
        None => vec![],
    };
    principals.set_trusted_issuers(issuers);
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

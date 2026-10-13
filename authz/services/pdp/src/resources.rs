//! Resources from awzy-context (demo plan step 3, ADR 0044).
//!
//! Resources can number in the millions, so unlike principals they aren't held as a snapshot.
//! Enforcement points send a resource's ID; the PDP loads the resource and its parent folders
//! from awzy-context on first use and keeps them in a local cache for a short time. What
//! awzy-context says replaces anything the enforcement point sent for those entities.
//!
//! With the entities comes who is assigned to work on the resource (ADR 0046): everyone on an
//! open work item that links it or a folder above it. The PDP turns that into `context.assigned`
//! for each principal it checks.
//!
//! A resource awzy-context doesn't know yet keeps the enforcement point's entity if it sent one;
//! otherwise it gets the default tier with `unclassified` set, so policies can challenge it.
//! awzy-context's change log tells the PDP which cached resources to drop as they change (ADR
//! 0077), including every one under a changed folder. While that feed is heard from, entries
//! live longer; when it isn't, they expire as they did before it.
//!
//! If awzy-context can't be reached, an expired entry is still used, but only for as long as its
//! sensitivity tier allows (ADR 0119): a stale attribute matters more on a sensitive resource.
//! Past that, or with nothing cached, the check fails and the enforcement point uses its fallback
//! (ADR 0030): deciding a crown-jewel document as "internal" because awzy-context was down would
//! be worse.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::{Duration, Instant},
};

use awzy_model::{entity_type, AttributesFrom, EntityRef};
use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::{json, Value};

/// The tier a resource awzy-context doesn't know gets. Matches awzy-context's default for one
/// it knows but hasn't classified.
pub const DEFAULT_SENSITIVITY: &str = "internal";

/// How long a loaded resource is used before it's loaded again. Within the plan's 60-second
/// target for an attribute change to reach every PDP.
const TTL: Duration = Duration::from_secs(30);
/// How long one is used while awzy-context's change log is heard from, which drops it sooner if
/// it changes. A safety net, not the freshness path.
const FEED_TTL: Duration = Duration::from_secs(300);
/// The change log counts as heard from for this long after its last answer.
const FEED_HEALTHY: Duration = Duration::from_secs(60);
/// How long "awzy-context doesn't know it" is remembered: short, since a connector may be about
/// to write it.
const UNKNOWN_TTL: Duration = Duration::from_secs(5);
/// Entries kept before expired ones are dropped, and then everything if none had expired.
const CAPACITY: usize = 100_000;
const TIMEOUT: Duration = Duration::from_secs(1);

/// How long an expired entry may still be used while awzy-context can't be reached, by the
/// resource's sensitivity tier (ADR 0119). A tier not listed, and a resource awzy-context didn't
/// know, get the shortest.
#[derive(Debug, Clone, PartialEq)]
pub struct StaleLimits(HashMap<String, Duration>);

impl Default for StaleLimits {
    fn default() -> Self {
        let mins = |m: u64| Duration::from_secs(m * 60);
        Self(HashMap::from([
            ("public".into(), mins(60)),
            ("internal".into(), mins(60)),
            ("confidential".into(), mins(15)),
            ("crown_jewels".into(), mins(5)),
        ]))
    }
}

impl StaleLimits {
    /// `tier=seconds` pairs, comma-separated (`AWZY_PDP_RESOURCES_MAX_STALE_SECS`), over the
    /// defaults.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let mut limits = Self::default();
        for pair in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (tier, secs) =
                pair.split_once('=').ok_or_else(|| anyhow::anyhow!("{pair:?} isn't tier=seconds"))?;
            anyhow::ensure!(awzy_model::TIERS.contains(&tier), "{tier:?} isn't a sensitivity tier");
            limits.0.insert(tier.to_string(), Duration::from_secs(secs.parse()?));
        }
        Ok(limits)
    }

    fn of(&self, loaded: Option<&Loaded>) -> Duration {
        let shortest = self.0.values().min().copied().unwrap_or_default();
        loaded.and_then(|l| l.tier.as_ref()).and_then(|t| self.0.get(t)).copied().unwrap_or(shortest)
    }
}

/// `GET /v1/resources/{id}/entities` from awzy-context.
#[derive(serde::Deserialize)]
struct EntitiesBody {
    entities: Vec<Value>,
    /// Absent from an awzy-context older than ADR 0046: no one is known to be assigned.
    #[serde(default)]
    assignees: Vec<String>,
    /// Absent from one older than ADR 0118: no work item is known to link it.
    #[serde(default)]
    work: Vec<WorkLink>,
}

#[derive(serde::Deserialize)]
struct WorkLink {
    id: String,
    #[serde(default)]
    assignees: Vec<String>,
}

/// Just the resource's tier, from its entity.
#[derive(serde::Deserialize, Default)]
struct TierOf {
    #[serde(default)]
    attrs: TierAttrs,
}

#[derive(serde::Deserialize, Default)]
struct TierAttrs {
    #[serde(default)]
    sensitivity: Option<String>,
}

/// What awzy-context says about a resource.
pub struct Loaded {
    /// The resource and its ancestors, nearest first.
    pub entities: Vec<Value>,
    /// The `userName`s and service-account IDs assigned to open work on it.
    pub assignees: Arc<HashSet<String>>,
    /// Each open work item that links it, by ID, with who is on it (ADR 0118).
    pub work: Arc<HashMap<String, HashSet<String>>>,
    /// The resource's sensitivity tier, which decides how long it may be used stale.
    pub tier: Option<String>,
}

/// The request's entities after the lookup, and, for a `Resource` checked with awzy-context,
/// who is assigned to work on it (empty for a resource awzy-context doesn't know).
pub struct Merged {
    pub entities: Value,
    pub assignees: Option<Arc<HashSet<String>>>,
    /// The open work items that link it, with who is on each (ADR 0118); empty when unknown.
    pub work: Arc<HashMap<String, HashSet<String>>>,
    /// Where the resource's attributes came from (ADR 0108).
    pub from: AttributesFrom,
}

struct Cached {
    at: Instant,
    /// `None` when awzy-context doesn't know the resource.
    entities: Option<Arc<Loaded>>,
}

impl Cached {
    fn fresh(&self, ttl: Duration, unknown_ttl: Duration) -> bool {
        self.at.elapsed() < if self.entities.is_some() { ttl } else { unknown_ttl }
    }
}

pub struct Resources {
    /// awzy-context's base URL; `None` when the PDP runs without it.
    endpoint: Option<String>,
    client: awzy_http::Client,
    /// This process's service token for awzy-context (ADR 0055).
    tokens: awzy_service_auth::TokenSource,
    cache: RwLock<HashMap<String, Cached>>,
    ttl: Duration,
    unknown_ttl: Duration,
    stale_limits: StaleLimits,
    started: Instant,
    /// Milliseconds after `started`, plus one, when the change log last answered; 0: never.
    feed_heard_ms: AtomicU64,
}

impl Resources {
    /// No awzy-context: the enforcement points' entities are used as they are.
    pub fn none() -> Self {
        Self::new(None, TTL, UNKNOWN_TTL)
    }

    pub fn from_context(endpoint: String) -> Self {
        Self::new(Some(endpoint), TTL, UNKNOWN_TTL)
    }

    fn new(endpoint: Option<String>, ttl: Duration, unknown_ttl: Duration) -> Self {
        Self {
            endpoint: endpoint.map(|e| e.trim_end_matches('/').to_string()),
            client: awzy_http::client(),
            tokens: awzy_service_auth::TokenSource::from_env(),
            cache: RwLock::default(),
            ttl,
            unknown_ttl,
            stale_limits: StaleLimits::default(),
            started: Instant::now(),
            feed_heard_ms: AtomicU64::new(0),
        }
    }

    pub fn with_stale_limits(mut self, limits: StaleLimits) -> Self {
        self.stale_limits = limits;
        self
    }

    /// Whether an expired entry may still stand in while awzy-context can't be reached.
    fn usable_stale(&self, c: &Cached) -> bool {
        c.at.elapsed() <= self.stale_limits.of(c.entities.as_deref())
    }

    /// How long a loaded resource is used: longer while the change log is heard from.
    fn ttl(&self) -> Duration {
        let heard = self.feed_heard_ms.load(Ordering::Acquire);
        let fresh = heard > 0 && self.started.elapsed() < Duration::from_millis(heard - 1) + FEED_HEALTHY;
        if fresh {
            FEED_TTL.max(self.ttl)
        } else {
            self.ttl
        }
    }

    /// Drops every cached resource that is, or sits under, one of `ids`.
    pub fn invalidate(&self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let changed: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
        cache.retain(|id, c| {
            let in_chain = c.entities.as_ref().is_some_and(|l| {
                l.entities.iter().any(|e| e["uid"]["id"].as_str().is_some_and(|i| changed.contains(i)))
            });
            !changed.contains(id.as_str()) && !in_chain
        });
    }

    /// Follows awzy-context's resource change log forever (ADR 0077), dropping what changed.
    pub async fn follow_changes(self: Arc<Self>) {
        let Some(endpoint) = self.endpoint.clone() else { return };
        let mut cursor: Option<i64> = None;
        loop {
            match self.changes_once(&endpoint, cursor).await {
                Ok((next, ids, reset)) => {
                    self.feed_heard_ms.store(self.started.elapsed().as_millis() as u64 + 1, Ordering::Release);
                    crate::metrics::RESOURCE_FEED.succeeded();
                    if reset {
                        self.cache.write().unwrap_or_else(|e| e.into_inner()).clear();
                        tracing::info!("resource change log reset; dropped every cached resource");
                    } else {
                        self.invalidate(&ids);
                    }
                    cursor = Some(next);
                }
                Err(error) => {
                    crate::metrics::RESOURCE_FEED.failed();
                    tracing::warn!(%error, "resource change log failed; cached resources expire on their own");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    /// One long-poll: the next cursor, the changed IDs, and whether to drop everything.
    async fn changes_once(&self, endpoint: &str, cursor: Option<i64>) -> anyhow::Result<(i64, Vec<String>, bool)> {
        let url = match cursor {
            Some(c) => format!("{endpoint}/v1/resources/changes?after={c}&wait=25"),
            None => format!("{endpoint}/v1/resources/changes"),
        };
        let req = crate::principals::context_get(&url, &self.tokens)?;
        let res = tokio::time::timeout(Duration::from_secs(35), self.client.request(req)).await??;
        anyhow::ensure!(res.status().is_success(), "{url} returned {}", res.status());
        let body: Value = serde_json::from_slice(&res.into_body().collect().await?.to_bytes())?;
        let next = body["cursor"].as_i64().ok_or_else(|| anyhow::anyhow!("no cursor from the change log"))?;
        let ids = body["ids"].as_array().into_iter().flatten().filter_map(|i| i.as_str().map(str::to_string)).collect();
        Ok((next, ids, body["reset"] == true))
    }

    /// The request's entities with the resource, and its ancestors, from awzy-context. Fails
    /// only when awzy-context can't be reached and nothing is cached for the resource.
    pub async fn merge(&self, request: Value, resource: &EntityRef) -> anyhow::Result<Merged> {
        let unchanged = |entities| {
            Ok(Merged { entities, assignees: None, work: Arc::default(), from: AttributesFrom::EnforcementPoint })
        };
        let Some(endpoint) = &self.endpoint else { return unchanged(request) };
        if resource.kind != entity_type::RESOURCE {
            return unchanged(request);
        }
        let Value::Array(mut entities) = request else { return unchanged(request) };
        let (assignees, work, from) = match self.lookup(endpoint, &resource.id).await? {
            Some(loaded) => {
                entities.retain(|e| !loaded.entities.iter().any(|c| c["uid"] == e["uid"]));
                entities.extend(loaded.entities.iter().cloned());
                (loaded.assignees.clone(), loaded.work.clone(), AttributesFrom::AwzyContext)
            }
            None => {
                let uid = json!({ "type": entity_type::RESOURCE, "id": resource.id });
                if entities.iter().any(|e| e["uid"] == uid) {
                    (Arc::default(), Arc::default(), AttributesFrom::EnforcementPoint)
                } else {
                    entities.push(json!({
                        "uid": uid,
                        "attrs": { "sensitivity": DEFAULT_SENSITIVITY, "unclassified": true },
                        "parents": [],
                    }));
                    (Arc::default(), Arc::default(), AttributesFrom::Default)
                }
            }
        };
        Ok(Merged { entities: Value::Array(entities), assignees: Some(assignees), work, from })
    }

    async fn lookup(&self, endpoint: &str, id: &str) -> anyhow::Result<Option<Arc<Loaded>>> {
        let stale = {
            let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
            match cache.get(id) {
                Some(c) if c.fresh(self.ttl(), self.unknown_ttl) => {
                    crate::metrics::RESOURCE_CACHE_HITS.inc();
                    return Ok(c.entities.clone());
                }
                Some(c) if self.usable_stale(c) => Some(c.entities.clone()),
                _ => None,
            }
        };
        match self.fetch(endpoint, id).await {
            Ok(entities) => {
                crate::metrics::RESOURCE_FETCHES.inc();
                self.store(id, entities.clone());
                Ok(entities)
            }
            Err(error) => match stale {
                Some(entities) => {
                    crate::metrics::RESOURCE_STALE_SERVED.inc();
                    tracing::warn!(%error, resource = id, "awzy-context unreachable; using the cached resource");
                    Ok(entities)
                }
                None => Err(error),
            },
        }
    }

    async fn fetch(&self, endpoint: &str, id: &str) -> anyhow::Result<Option<Arc<Loaded>>> {
        let url = format!("{endpoint}/v1/resources/{}/entities", awzy_http::encode(id));
        let req = crate::principals::context_get(&url, &self.tokens)?;
        let res = tokio::time::timeout(TIMEOUT, self.client.request(req)).await??;
        match res.status().as_u16() {
            404 => Ok(None),
            200 => {
                let body: EntitiesBody = serde_json::from_slice(&res.into_body().collect().await?.to_bytes())
                    .map_err(|e| anyhow::anyhow!("no entities for resource {id}: {e}"))?;
                let work = body.work.into_iter().map(|w| (w.id, w.assignees.into_iter().collect())).collect();
                // The resource itself comes first.
                let tier =
                    body.entities.first().and_then(|e| TierOf::deserialize(e).ok()).and_then(|t| t.attrs.sensitivity);
                Ok(Some(Arc::new(Loaded {
                    tier,
                    entities: body.entities,
                    assignees: Arc::new(body.assignees.into_iter().collect()),
                    work: Arc::new(work),
                })))
            }
            status => anyhow::bail!("{url} returned {status}"),
        }
    }

    fn store(&self, id: &str, entities: Option<Arc<Loaded>>) {
        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= CAPACITY && !cache.contains_key(id) {
            // What can't stand in during an outage goes first, then what's merely expired.
            cache.retain(|_, c| self.usable_stale(c));
            if cache.len() >= CAPACITY {
                cache.retain(|_, c| c.fresh(self.ttl(), self.unknown_ttl));
            }
            if cache.len() >= CAPACITY {
                cache.clear();
            }
        }
        cache.insert(id.to_string(), Cached { at: Instant::now(), entities });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::{extract::Path, routing::get, Json, Router};

    use super::*;

    #[test]
    fn a_change_drops_the_resource_and_everything_under_it() {
        let r = Resources::new(Some("http://context".into()), TTL, UNKNOWN_TTL);
        let loaded = |ids: &[&str]| {
            let entities = ids.iter().map(|id| json!({ "uid": { "type": "Resource", "id": id } })).collect();
            Some(Arc::new(Loaded { entities, assignees: Arc::default(), work: Arc::default(), tier: None }))
        };
        r.store("doc", loaded(&["doc", "folder"]));
        r.store("folder", loaded(&["folder"]));
        r.store("elsewhere", loaded(&["elsewhere"]));
        r.store("not-yet", None);
        r.invalidate(&["folder".into(), "not-yet".into()]);
        let cache = r.cache.read().unwrap();
        let mut left: Vec<&str> = cache.keys().map(String::as_str).collect();
        left.sort();
        assert_eq!(left, ["elsewhere"], "the folder, the document in it, and the one just written");
    }

    /// A fake awzy-context that knows a document in a folder, and counts requests.
    async fn context() -> (String, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let app = Router::new().route(
            "/v1/resources/{id}/entities",
            get(move |Path(id): Path<String>| {
                counted.fetch_add(1, Ordering::SeqCst);
                async move {
                    if id != "drive:plans/fy27" {
                        return Err(axum::http::StatusCode::NOT_FOUND);
                    }
                    Ok(Json(json!({ "assignees": ["nora.patel@larkspur.example"], "entities": [
                        { "uid": { "type": "Resource", "id": "drive:plans/fy27" },
                          "attrs": { "sensitivity": "crown_jewels", "unclassified": false },
                          "parents": [{ "type": "Resource", "id": "drive:plans" }] },
                        { "uid": { "type": "Resource", "id": "drive:plans" },
                          "attrs": { "sensitivity": "internal", "unclassified": true }, "parents": [] },
                    ]})))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), calls)
    }

    fn resource(id: &str) -> EntityRef {
        EntityRef::new("Resource", id)
    }

    fn ids(v: &Value) -> Vec<&str> {
        v.as_array().unwrap().iter().map(|e| e["uid"]["id"].as_str().unwrap()).collect()
    }

    #[tokio::test]
    async fn awzy_context_wins_over_what_the_enforcement_point_sent() {
        let (url, calls) = context().await;
        let r = Resources::from_context(url);
        let sent = json!([
            { "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "Resource", "id": "drive:plans/fy27" }, "attrs": { "sensitivity": "public" }, "parents": [] },
        ]);
        let merged = r.merge(sent.clone(), &resource("drive:plans/fy27")).await.unwrap();
        assert_eq!(ids(&merged.entities), ["bob", "drive:plans/fy27", "drive:plans"], "the resource and its folder");
        assert_eq!(merged.entities[1]["attrs"]["sensitivity"], "crown_jewels");
        assert_eq!(merged.from, AttributesFrom::AwzyContext, "awzy-context's replaced what was sent");
        let assignees = merged.assignees.unwrap();
        assert!(assignees.contains("nora.patel@larkspur.example") && assignees.len() == 1);

        r.merge(sent, &resource("drive:plans/fy27")).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the second check is served from the cache");
    }

    #[tokio::test]
    async fn an_unknown_resource_is_unclassified_unless_the_enforcement_point_described_it() {
        let (url, calls) = context().await;
        let r = Resources::from_context(url);
        let merged = r.merge(json!([]), &resource("drive:new")).await.unwrap();
        assert_eq!(merged.entities[0]["attrs"], json!({ "sensitivity": DEFAULT_SENSITIVITY, "unclassified": true }));
        assert_eq!(merged.from, AttributesFrom::Default);
        assert!(merged.assignees.unwrap().is_empty(), "known to have no one assigned");

        let sent = json!([{ "uid": { "type": "Resource", "id": "drive:new" }, "attrs": { "sensitivity": "confidential" }, "parents": [] }]);
        let described = r.merge(sent.clone(), &resource("drive:new")).await.unwrap();
        assert_eq!((described.entities, described.from), (sent, AttributesFrom::EnforcementPoint));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "not knowing it is cached too");
    }

    #[tokio::test]
    async fn unreachable_fails_unless_something_is_cached() {
        let (url, _) = context().await;
        let r = Resources::new(Some(url), Duration::ZERO, Duration::ZERO);
        r.merge(json!([]), &resource("drive:plans/fy27")).await.unwrap();

        let down = Resources::new(Some("http://127.0.0.1:1".into()), Duration::ZERO, Duration::ZERO);
        assert!(down.merge(json!([]), &resource("drive:plans/fy27")).await.is_err(), "fails closed");
        down.store("drive:plans/fy27", r.cache.read().unwrap()["drive:plans/fy27"].entities.clone());
        let merged = down.merge(json!([]), &resource("drive:plans/fy27")).await.unwrap();
        assert_eq!(merged.entities[0]["attrs"]["sensitivity"], "crown_jewels", "an expired entry beats no answer");
    }

    #[tokio::test]
    async fn a_stale_resource_stands_in_only_as_long_as_its_tier_allows() {
        let (url, _) = context().await;
        let r = Resources::new(Some(url), Duration::ZERO, Duration::ZERO);
        r.merge(json!([]), &resource("drive:plans/fy27")).await.unwrap();
        let cached = r.cache.read().unwrap()["drive:plans/fy27"].entities.clone();
        assert_eq!(cached.as_ref().unwrap().tier.as_deref(), Some("crown_jewels"));

        let limits = StaleLimits::parse("crown_jewels=0").unwrap();
        let down = Resources::new(Some("http://127.0.0.1:1".into()), Duration::ZERO, Duration::ZERO)
            .with_stale_limits(limits);
        down.store("drive:plans/fy27", cached.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(down.merge(json!([]), &resource("drive:plans/fy27")).await.is_err(), "past its tier's limit");

        let lenient = Resources::new(Some("http://127.0.0.1:1".into()), Duration::ZERO, Duration::ZERO);
        lenient.store("drive:plans/fy27", cached);
        assert!(lenient.merge(json!([]), &resource("drive:plans/fy27")).await.is_ok(), "within 5 minutes");
    }

    #[test]
    fn stale_limits_are_set_by_tier() {
        let limits = StaleLimits::parse("confidential=60, public=7200").unwrap();
        let loaded = |tier: &str| Loaded {
            entities: vec![],
            assignees: Arc::default(),
            work: Arc::default(),
            tier: Some(tier.into()),
        };
        assert_eq!(limits.of(Some(&loaded("confidential"))), Duration::from_secs(60));
        assert_eq!(limits.of(Some(&loaded("public"))), Duration::from_secs(7200));
        assert_eq!(limits.of(Some(&loaded("crown_jewels"))), Duration::from_secs(300), "the default stays");
        assert_eq!(limits.of(Some(&loaded("secret"))), Duration::from_secs(60), "an unknown tier gets the shortest");
        assert_eq!(limits.of(None), Duration::from_secs(60), "so does a resource awzy-context didn't know");
        assert!(StaleLimits::parse("top_secret=1").is_err());
        assert!(StaleLimits::parse("public").is_err());
    }

    #[tokio::test]
    async fn other_resource_types_and_no_awzy_context_are_left_alone() {
        let sent = json!([{ "uid": { "type": "McpTool", "id": "drive/search" }, "attrs": { "sensitivity": "internal" }, "parents": [] }]);
        let unused = Resources::from_context("http://127.0.0.1:1".into());
        let tool = unused.merge(sent.clone(), &EntityRef::new("McpTool", "drive/search")).await.unwrap();
        assert_eq!((tool.entities, tool.assignees), (sent.clone(), None), "assignment unknown");
        assert_eq!(tool.from, AttributesFrom::EnforcementPoint);
        // An app-owned type (ADR 0108) is the enforcement point's, and never looked up.
        let customer =
            json!([{ "uid": { "type": "Customer", "id": "c-1" }, "attrs": { "tenant": "acme" }, "parents": [] }]);
        let merged = unused.merge(customer.clone(), &EntityRef::new("Customer", "c-1")).await.unwrap();
        assert_eq!((merged.entities, merged.from), (customer, AttributesFrom::EnforcementPoint));
        let none = Resources::none().merge(sent.clone(), &resource("drive:x")).await.unwrap();
        assert_eq!((none.entities, none.assignees), (sent, None));
    }

    #[test]
    fn ids_are_one_path_segment() {
        assert_eq!(awzy_http::encode("synthetic:folder:sales/accounts"), "synthetic%3Afolder%3Asales%2Faccounts");
    }
}

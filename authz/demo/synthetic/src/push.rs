//! Sends the company to awzy-context: people and groups over SCIM, as an IdP would, then the
//! service accounts through awzy-context's API, as the client portal's Directory page does.
//! Then the world: resources and work items through the batch API, as a connector writes them.
//! Safe to run again: existing people, groups, resources, and work items are replaced, not
//! duplicated.

use std::{collections::HashMap, time::Duration};

use anyhow::{bail, Context};
use axum::http::{header, Method, Request, StatusCode};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use serde_json::{json, Value};

use crate::{
    org::{Company, Profile},
    scim,
    world::{self, Resource},
};

/// The most items one batch request carries: awzy-context's limit.
pub const BATCH: usize = 1000;

type HttpClient = awzy_http::Client;

const TIMEOUT: Duration = Duration::from_secs(30);

/// awzy-context at `base_url` (e.g. http://127.0.0.1:8084), with the IdP's SCIM bearer token.
pub struct ContextClient {
    base_url: String,
    scim_token: String,
    http: HttpClient,
    tokens: awzy_service_auth::TokenSource,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub humans: usize,
    pub groups: usize,
    pub service_accounts: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct WorldSummary {
    pub resources: usize,
    pub work_items: usize,
}

impl ContextClient {
    pub fn new(base_url: &str, scim_token: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            scim_token: scim_token.to_string(),
            http: awzy_http::client(),
            tokens: awzy_service_auth::TokenSource::from_env(),
        }
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        scim: bool,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let mut req = Request::builder().method(method.clone()).uri(format!("{}{path}", self.base_url));
        if scim {
            req = req.header(header::AUTHORIZATION, format!("Bearer {}", self.scim_token));
            req = req.header(header::CONTENT_TYPE, "application/scim+json");
        } else {
            req = req
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-awzy-actor", "AwzyService::awzy-synthetic");
            // This Job's service token for awzy-context (ADR 0055); SCIM uses the IdP's token.
            if let Some(authorization) = self.tokens.authorization("awzy-context")? {
                req = req.header(header::AUTHORIZATION, authorization);
            }
        }
        let body = body.map(|b| Bytes::from(b.to_string())).unwrap_or_default();
        let res = tokio::time::timeout(TIMEOUT, self.http.request(req.body(Full::new(body))?))
            .await
            .with_context(|| format!("{method} {path} timed out"))??;
        let status = res.status();
        let bytes = res.into_body().collect().await?.to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()));
        Ok((status, value))
    }

    /// A GET on awzy-context's own API (not SCIM): the status and JSON body.
    pub(crate) async fn get(&self, path: &str) -> anyhow::Result<(StatusCode, Value)> {
        self.send(Method::GET, path, None, false).await
    }

    /// Creates the resource, or replaces it if one with the same `attr` value exists. Returns
    /// its SCIM ID.
    async fn upsert(&self, kind: &str, attr: &str, resource: &Value) -> anyhow::Result<String> {
        let key = resource[attr].as_str().context("resource without its key attribute")?;
        let (status, body) = self.send(Method::POST, &format!("/scim/v2/{kind}"), Some(resource), true).await?;
        if status == StatusCode::CREATED {
            return id_of(&body);
        }
        if status != StatusCode::CONFLICT {
            bail!("creating {kind} {key}: {status}: {body}");
        }
        let filter = encode(&format!("{attr} eq \"{key}\""));
        let (status, list) = self.send(Method::GET, &format!("/scim/v2/{kind}?filter={filter}"), None, true).await?;
        let id = match list["Resources"].get(0) {
            Some(existing) if status.is_success() => id_of(existing)?,
            _ => bail!("{kind} {key} exists but can't be found: {status}: {list}"),
        };
        let (status, body) = self.send(Method::PUT, &format!("/scim/v2/{kind}/{id}"), Some(resource), true).await?;
        if !status.is_success() {
            bail!("replacing {kind} {key}: {status}: {body}");
        }
        Ok(id)
    }

    /// People first (managers before their reports, so each manager's ID is known), then groups
    /// (contained groups first), then service accounts, whose owners must exist.
    pub async fn push(&self, company: &Company) -> anyhow::Result<Summary> {
        let mut ids: HashMap<String, String> = HashMap::new();
        for p in &company.humans {
            let manager_id = p.manager.as_ref().and_then(|m| ids.get(m)).map(String::as_str);
            let id = self.upsert("Users", "userName", &scim::user(p, manager_id)).await?;
            ids.insert(p.user_name.clone(), id);
        }
        tracing::info!(humans = company.humans.len(), "people provisioned");

        let mut group_ids: HashMap<String, String> = HashMap::new();
        for g in &company.groups {
            let humans = g.humans.iter().map(|p| ids[p].clone());
            let members: Vec<String> = humans.chain(g.groups.iter().map(|n| group_ids[n].clone())).collect();
            let id = self.upsert("Groups", "displayName", &scim::group(&g.name, &members)).await?;
            group_ids.insert(g.name.clone(), id);
        }
        tracing::info!(groups = company.groups.len(), "groups provisioned");

        for sa in &company.service_accounts {
            let body = json!({ "owner": sa.owner, "ai": sa.ai, "groups": sa.groups, "description": sa.description });
            let path = format!("/v1/service-accounts/{}", sa.id);
            let (status, res) = self.send(Method::PUT, &path, Some(&body), false).await?;
            if !status.is_success() {
                bail!("registering service account {}: {status}: {res}", sa.id);
            }
        }
        tracing::info!(service_accounts = company.service_accounts.len(), "service accounts registered");

        Ok(Summary {
            humans: company.humans.len(),
            groups: company.groups.len(),
            service_accounts: company.service_accounts.len(),
        })
    }
}

impl ContextClient {
    async fn batch(&self, path: &str, key: &str, items: Vec<Value>) -> anyhow::Result<usize> {
        let n = items.len();
        let body = Value::Object([(key.to_string(), Value::Array(items))].into_iter().collect());
        let (status, res) = self.send(Method::POST, path, Some(&body), false).await?;
        if !status.is_success() {
            bail!("writing {n} items to {path}: {status}: {res}");
        }
        Ok(n)
    }

    async fn resources(&self, resources: &[Resource]) -> anyhow::Result<usize> {
        let mut n = 0;
        for chunk in resources.chunks(BATCH) {
            let items = chunk.iter().map(Resource::to_input).collect();
            n += self.batch("/v1/resources/batch", "resources", items).await?;
        }
        Ok(n)
    }

    /// Folders, repositories, and the scenarios' documents, then everyone's documents a batch at
    /// a time (the scale profile's million are never all in memory), then work items. Run after
    /// [`Self::push`]: owners and assignees are the directory's people and service accounts.
    pub async fn push_world(&self, company: &Company, profile: Profile) -> anyhow::Result<WorldSummary> {
        let mut fixed = world::folders_and_repositories(company);
        fixed.extend(world::scenario_resources(company));
        let mut resources = self.resources(&fixed).await?;

        let mut pending: Vec<Resource> = Vec::with_capacity(2 * BATCH);
        for index in 0..company.humans.len() {
            pending.extend(world::human_documents(company, profile, company.seed, index));
            if pending.len() >= BATCH {
                let full = pending.len() / BATCH * BATCH;
                resources += self.resources(&pending[..full]).await?;
                pending = pending.split_off(full);
            }
            if index % 1000 == 999 {
                tracing::info!(humans = index + 1, resources, "documents sent");
            }
        }
        resources += self.resources(&pending).await?;
        tracing::info!(resources, "resources written");

        let work = world::work_items(company, profile, company.seed);
        for chunk in work.chunks(BATCH) {
            let items = chunk.iter().map(world::WorkItem::to_input).collect();
            self.batch("/v1/work-items/batch", "work_items", items).await?;
        }
        tracing::info!(work_items = work.len(), "work items written");
        Ok(WorldSummary { resources, work_items: work.len() })
    }
}

fn id_of(resource: &Value) -> anyhow::Result<String> {
    resource["id"].as_str().map(str::to_string).with_context(|| format!("no id in {resource}"))
}

/// Percent-encodes a SCIM filter for a query string.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        extract::{Path, Query, State},
        routing::{get, put},
        Json, Router,
    };

    use super::*;
    use crate::org::{generate, Profile};

    /// A fake awzy-context: SCIM create, filter, and replace, and the service-account API.
    #[derive(Default)]
    struct Fake {
        users: HashMap<String, String>,
        groups: HashMap<String, Value>,
        service_accounts: HashMap<String, Value>,
        replaced: usize,
        next: usize,
        resources: HashMap<String, Value>,
        work_items: HashMap<String, Value>,
        largest_batch: usize,
    }

    type Shared = Arc<Mutex<Fake>>;

    async fn create(
        State(f): State<Shared>,
        Path(kind): Path<String>,
        Json(r): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let mut f = f.lock().unwrap();
        let key = if kind == "Users" { r["userName"].clone() } else { r["displayName"].clone() };
        let key = key.as_str().unwrap().to_string();
        let exists = if kind == "Users" { f.users.contains_key(&key) } else { f.groups.contains_key(&key) };
        if exists {
            return (StatusCode::CONFLICT, Json(json!({ "scimType": "uniqueness" })));
        }
        f.next += 1;
        let id = format!("id-{}", f.next);
        if kind == "Users" {
            f.users.insert(key, id.clone());
        } else {
            f.groups.insert(key, json!({ "id": id, "members": r["members"] }));
        }
        (StatusCode::CREATED, Json(json!({ "id": id })))
    }

    async fn find(
        State(f): State<Shared>,
        Path(kind): Path<String>,
        Query(q): Query<HashMap<String, String>>,
    ) -> Json<Value> {
        let f = f.lock().unwrap();
        let key = q["filter"].split('"').nth(1).unwrap().to_string();
        let id = if kind == "Users" {
            f.users.get(&key).cloned()
        } else {
            f.groups.get(&key).map(|g| g["id"].as_str().unwrap().to_string())
        };
        Json(json!({ "Resources": id.map(|id| vec![json!({ "id": id })]).unwrap_or_default() }))
    }

    async fn replace(State(f): State<Shared>) -> StatusCode {
        f.lock().unwrap().replaced += 1;
        StatusCode::OK
    }

    async fn register(State(f): State<Shared>, Path(id): Path<String>, Json(b): Json<Value>) -> StatusCode {
        let mut f = f.lock().unwrap();
        let owner_known = f.users.contains_key(b["owner"].as_str().unwrap());
        f.service_accounts.insert(id, b);
        if owner_known {
            StatusCode::OK
        } else {
            StatusCode::BAD_REQUEST
        }
    }

    async fn batch(State(f): State<Shared>, Path(kind): Path<String>, Json(b): Json<Value>) -> StatusCode {
        let mut f = f.lock().unwrap();
        let key = if kind == "resources" { "resources" } else { "work_items" };
        let items = b[key].as_array().unwrap();
        f.largest_batch = f.largest_batch.max(items.len());
        if items.len() > BATCH {
            return StatusCode::BAD_REQUEST;
        }
        for item in items {
            let id = item["id"].as_str().unwrap().to_string();
            if kind == "resources" {
                f.resources.insert(id, item.clone());
            } else {
                f.work_items.insert(id, item.clone());
            }
        }
        StatusCode::OK
    }

    async fn fake() -> (String, Shared) {
        let state: Shared = Arc::default();
        let app = Router::new()
            .route("/scim/v2/{kind}", get(find).post(create))
            .route("/scim/v2/{kind}/{id}", put(replace))
            .route("/v1/service-accounts/{id}", put(register))
            .route("/v1/{kind}/batch", axum::routing::post(batch))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), state)
    }

    #[tokio::test]
    async fn provisions_the_company_and_can_run_again() {
        let (url, state) = fake().await;
        let company = generate(Profile::Demo, 1);
        let client = ContextClient::new(&url, "token");

        let summary = client.push(&company).await.unwrap();
        assert_eq!(summary.humans, company.humans.len());
        {
            let f = state.lock().unwrap();
            assert_eq!(f.users.len(), company.humans.len());
            assert_eq!(f.groups.len(), company.groups.len());
            assert_eq!(f.service_accounts.len(), 4);
            assert_eq!(f.replaced, 0);
            // Department groups hold their teams' groups by ID.
            let engineering = &f.groups["engineering"]["members"];
            assert_eq!(engineering.as_array().unwrap().len(), 3);
        }

        // A second run replaces instead of duplicating.
        client.push(&company).await.unwrap();
        let f = state.lock().unwrap();
        assert_eq!(f.users.len(), company.humans.len());
        assert_eq!(f.replaced, company.humans.len() + company.groups.len());
    }

    #[tokio::test]
    async fn sends_the_world_in_batches() {
        let (url, state) = fake().await;
        let company = generate(Profile::Demo, 1);
        let summary = ContextClient::new(&url, "token").push_world(&company, Profile::Demo).await.unwrap();
        let f = state.lock().unwrap();
        assert_eq!((f.resources.len(), f.work_items.len()), (summary.resources, summary.work_items));
        assert!(summary.resources > 3_000, "{summary:?}");
        assert_eq!(f.largest_batch, BATCH);
        let plan = &f.resources["synthetic:doc:strategic-plan-fy27"];
        assert_eq!((plan["source"].as_str(), plan["sensitivity"].as_str()), (Some("synthetic"), Some("crown_jewels")));
        assert!(f.resources.values().any(|r| r["sensitivity"].is_null()), "unlabeled documents arrive unclassified");
        assert!(f.resources.values().all(|r| r.get("truth").is_none()));
    }

    #[test]
    fn filters_are_encoded() {
        assert_eq!(encode("userName eq \"a@b.example\""), "userName%20eq%20%22a%40b.example%22");
    }
}

//! The directory: humans and groups the client's IdP provisions, and the service accounts the
//! client's admins register (ADR 0031, ADR 0033). Read from awzy-context; people and groups are
//! read-only here because the IdP owns them.

use async_graphql::{Context, InputObject, Object, Result, SimpleObject};
use awzy_portal::{Actor, UtcTimestamp};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use crate::access::{require, PortalAction, PortalArea};

/// awzy-context's HTTP API.
#[derive(Clone)]
pub struct ContextClient {
    base: String,
    client: awzy_http::Client,
    /// This portal's service token for awzy-context (ADR 0055).
    tokens: awzy_service_auth::TokenSource,
}

impl ContextClient {
    /// awzy-context's base URL.
    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn new(base: String) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            client: awzy_http::client(),
            tokens: awzy_service_auth::TokenSource::from_env(),
        }
    }

    pub(crate) async fn call(
        &self,
        method: &str,
        path: &str,
        actor: Option<&str>,
        body: Option<Value>,
    ) -> anyhow::Result<(u16, Bytes)> {
        let mut req = awzy_http::Request::builder()
            .method(method)
            .uri(format!("{}{path}", self.base))
            .header("content-type", "application/json");
        if let Some(a) = actor {
            req = req.header("x-awzy-actor", a);
        }
        // This portal's service token for awzy-context (ADR 0055).
        if let Some(authorization) = self.tokens.authorization("awzy-context")? {
            req = req.header("authorization", authorization);
        }
        let req = req.body(body.map(|b| awzy_http::Body::from(b.to_string())).unwrap_or_default())?;
        let res = awzy_http::send(&self.client, req, std::time::Duration::from_secs(10)).await?;
        Ok((res.status.as_u16(), res.body))
    }

    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        let (status, body) = self.call("GET", path, None, None).await?;
        anyhow::ensure!(status == 200, "awzy-context answered {status}: {}", String::from_utf8_lossy(&body));
        Ok(serde_json::from_slice(&body)?)
    }
}

pub(crate) fn context<'a>(ctx: &Context<'a>) -> Result<&'a ContextClient> {
    ctx.data::<crate::Services>()?
        .context
        .as_ref()
        .ok_or_else(|| "awzy-context is not configured (AWZY_CONTEXT_ADDR)".into())
}

// ---- What awzy-context sends

#[derive(Deserialize)]
struct WireHuman {
    user_name: String,
    display_name: Option<String>,
    active: bool,
    groups: Vec<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct WireGroup {
    name: String,
    humans: i64,
    groups: i64,
    parents: Vec<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct WireSummary {
    humans: i64,
    active_humans: i64,
    groups: i64,
    service_accounts: i64,
    last_provisioned_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct WireServiceAccount {
    id: String,
    owner: String,
    ai: bool,
    groups: Vec<String>,
    description: String,
    created_by: String,
    created_at: DateTime<Utc>,
    updated_by: String,
    updated_at: DateTime<Utc>,
}

// ---- GraphQL

/// A person, as the IdP provisioned them: a `Human` in policies.
#[derive(SimpleObject)]
pub struct Human {
    /// Their ID in policies and tokens.
    pub user_name: String,
    pub display_name: Option<String>,
    /// False once deprovisioned: denied everything.
    pub active: bool,
    /// Groups they're directly in.
    pub groups: Vec<String>,
    pub updated_at: UtcTimestamp,
}

#[derive(SimpleObject)]
pub struct DirectoryGroup {
    /// The group's ID in policies, e.g. Group::"security".
    pub name: String,
    /// Humans directly in it.
    pub humans: i64,
    /// Groups nested in it.
    pub groups: i64,
    /// Groups it is nested in.
    pub parents: Vec<String>,
    pub updated_at: UtcTimestamp,
}

#[derive(SimpleObject)]
pub struct DirectorySummary {
    pub humans: i64,
    pub active_humans: i64,
    pub groups: i64,
    pub service_accounts: i64,
    /// When the IdP last changed a person or group.
    pub last_provisioned_at: Option<UtcTimestamp>,
}

/// An agent, backend service, or scheduled job, with the person accountable for it.
#[derive(SimpleObject)]
pub struct ServiceAccount {
    /// Its ID in policies and tokens.
    pub id: String,
    /// The accountable person's userName.
    pub owner: String,
    pub ai: bool,
    pub groups: Vec<String>,
    pub description: String,
    pub created_by: Option<Actor>,
    pub created_at: UtcTimestamp,
    pub updated_by: Option<Actor>,
    pub updated_at: UtcTimestamp,
}

impl From<WireServiceAccount> for ServiceAccount {
    fn from(w: WireServiceAccount) -> Self {
        Self {
            id: w.id,
            owner: w.owner,
            ai: w.ai,
            groups: w.groups,
            description: w.description,
            created_by: Actor::from_text(&w.created_by),
            created_at: UtcTimestamp(w.created_at),
            updated_by: Actor::from_text(&w.updated_by),
            updated_at: UtcTimestamp(w.updated_at),
        }
    }
}

#[derive(Default)]
pub struct DirectoryQuery;

#[Object]
impl DirectoryQuery {
    async fn directory_summary(&self, ctx: &Context<'_>) -> Result<DirectorySummary> {
        require(ctx, PortalAction::View, PortalArea::Directory).await?;
        let s: WireSummary = context(ctx)?.get("/v1/directory/summary").await?;
        Ok(DirectorySummary {
            humans: s.humans,
            active_humans: s.active_humans,
            groups: s.groups,
            service_accounts: s.service_accounts,
            last_provisioned_at: s.last_provisioned_at.map(UtcTimestamp),
        })
    }

    /// Humans by userName; `search` matches userName or display name.
    async fn humans(
        &self,
        ctx: &Context<'_>,
        search: Option<String>,
        #[graphql(default = 200)] first: i32,
    ) -> Result<Vec<Human>> {
        require(ctx, PortalAction::View, PortalArea::Directory).await?;
        let mut path = format!("/v1/directory/humans?limit={}", first.clamp(1, 1000));
        if let Some(s) = search.filter(|s| !s.trim().is_empty()) {
            path.push_str(&format!("&search={}", urlencode(s.trim())));
        }
        #[derive(Deserialize)]
        struct Wire {
            humans: Vec<WireHuman>,
        }
        let w: Wire = context(ctx)?.get(&path).await?;
        Ok(w.humans
            .into_iter()
            .map(|p| Human {
                user_name: p.user_name,
                display_name: p.display_name,
                active: p.active,
                groups: p.groups,
                updated_at: UtcTimestamp(p.updated_at),
            })
            .collect())
    }

    async fn groups(&self, ctx: &Context<'_>) -> Result<Vec<DirectoryGroup>> {
        require(ctx, PortalAction::View, PortalArea::Directory).await?;
        #[derive(Deserialize)]
        struct Wire {
            groups: Vec<WireGroup>,
        }
        let w: Wire = context(ctx)?.get("/v1/directory/groups").await?;
        Ok(w.groups
            .into_iter()
            .map(|g| DirectoryGroup {
                name: g.name,
                humans: g.humans,
                groups: g.groups,
                parents: g.parents,
                updated_at: UtcTimestamp(g.updated_at),
            })
            .collect())
    }

    async fn service_accounts(&self, ctx: &Context<'_>) -> Result<Vec<ServiceAccount>> {
        require(ctx, PortalAction::View, PortalArea::Directory).await?;
        #[derive(Deserialize)]
        struct Wire {
            service_accounts: Vec<WireServiceAccount>,
        }
        let w: Wire = context(ctx)?.get("/v1/service-accounts").await?;
        Ok(w.service_accounts.into_iter().map(Into::into).collect())
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(InputObject, Serialize)]
pub struct RegisterServiceAccountInput {
    /// Its ID in tokens: the `sub` of its own tokens, or `act.sub` when acting for a person.
    pub id: String,
    /// The accountable person's userName; must be provisioned by the IdP.
    pub owner: String,
    pub ai: bool,
    #[graphql(default)]
    pub groups: Vec<String>,
    #[graphql(default)]
    pub description: String,
}

#[derive(SimpleObject)]
pub struct RegisterServiceAccountPayload {
    pub service_account: ServiceAccount,
}

#[derive(InputObject)]
pub struct RemoveServiceAccountInput {
    pub id: String,
}

#[derive(SimpleObject)]
pub struct RemoveServiceAccountPayload {
    pub removed_id: String,
}

#[derive(Default)]
pub struct DirectoryMutation;

#[Object]
impl DirectoryMutation {
    /// Registers a service account, or changes one. Until it's registered, the PDP denies it.
    async fn register_service_account(
        &self,
        ctx: &Context<'_>,
        input: RegisterServiceAccountInput,
    ) -> Result<RegisterServiceAccountPayload> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Directory).await?;
        let body = serde_json::to_value(&input)?;
        let (status, res) = context(ctx)?
            .call(
                "PUT",
                &format!("/v1/service-accounts/{}", urlencode(&input.id)),
                Some(&viewer.actor().to_string()),
                Some(body),
            )
            .await?;
        if status != 200 {
            return Err(String::from_utf8_lossy(&res).into_owned().into());
        }
        #[derive(Deserialize)]
        struct Wire {
            service_account: WireServiceAccount,
        }
        let w: Wire = serde_json::from_slice(&res)?;
        Ok(RegisterServiceAccountPayload { service_account: w.service_account.into() })
    }

    /// Removes a service account. From then on the PDP denies it.
    async fn remove_service_account(
        &self,
        ctx: &Context<'_>,
        input: RemoveServiceAccountInput,
    ) -> Result<RemoveServiceAccountPayload> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Directory).await?;
        let (status, res) = context(ctx)?
            .call(
                "DELETE",
                &format!("/v1/service-accounts/{}", urlencode(&input.id)),
                Some(&viewer.actor().to_string()),
                None,
            )
            .await?;
        match status {
            204 => Ok(RemoveServiceAccountPayload { removed_id: input.id }),
            _ => Err(String::from_utf8_lossy(&res).into_owned().into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        extract::Path,
        http::HeaderMap,
        routing::{get, put},
        Json, Router,
    };
    use serde_json::{json, Value};

    use crate::{testing, Services};

    /// A fake awzy-context that records registrations and who made them.
    async fn fake_context(seen: Arc<Mutex<Vec<(String, String, Value)>>>) -> String {
        let at = "2026-10-09T12:00:00Z";
        let app = Router::new()
            .route("/v1/directory/summary", get(move || async move { Json(json!({ "humans": 2, "active_humans": 1, "groups": 1, "service_accounts": 0, "last_provisioned_at": at })) }))
            .route(
                "/v1/directory/humans",
                get(move || async move {
                    Json(json!({ "humans": [{ "user_name": "alice", "display_name": "Alice", "active": true, "groups": ["security"], "updated_at": at }] }))
                }),
            )
            .route(
                "/v1/service-accounts/{id}",
                put(move |Path(id): Path<String>, headers: HeaderMap, Json(body): Json<Value>| {
                    let seen = seen.clone();
                    async move {
                        let actor = headers["x-awzy-actor"].to_str().unwrap().to_string();
                        seen.lock().unwrap().push((id.clone(), actor.clone(), body.clone()));
                        Json(json!({ "service_account": {
                            "id": id, "owner": body["owner"], "ai": body["ai"], "groups": body["groups"], "description": body["description"],
                            "created_by": actor, "created_at": at, "updated_by": actor, "updated_at": at } }))
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn directory_reads_and_service_account_changes_are_authorized() {
        let seen = Arc::new(Mutex::new(vec![]));
        let services = Services {
            pdp: Some(testing::pdp().await),
            context: Some(super::ContextClient::new(fake_context(seen.clone()).await)),
            ..Default::default()
        };
        let schema = crate::schema_with(services);
        let alice = testing::human("alice", "security");
        let bob = testing::human("bob", "engineering");

        let d = testing::run(&schema, &alice, "{ directorySummary { humans activeHumans lastProvisionedAt } humans(search: \"ali\") { userName groups active } }").await;
        assert_eq!(d["directorySummary"]["activeHumans"], 1);
        assert_eq!(d["humans"][0], json!({ "userName": "alice", "groups": ["security"], "active": true }));

        let register = r#"mutation { registerServiceAccount(input: { id: "sales-genie", owner: "bob", ai: true, groups: ["engineering"], description: "Drafts emails" }) { serviceAccount { id owner ai createdBy { type id } } } }"#;
        let r = testing::run(&schema, &alice, register).await;
        assert_eq!(
            r["registerServiceAccount"]["serviceAccount"],
            json!({ "id": "sales-genie", "owner": "bob", "ai": true, "createdBy": { "type": "Human", "id": "alice" } })
        );
        assert_eq!(seen.lock().unwrap()[0].1, "Human::alice", "awzy-context records who registered it");

        let err = testing::run_err(&schema, &bob, register).await;
        assert!(err.starts_with("bob may not administer directory"), "{err}");
        let err = testing::run_err(&schema, &bob, "{ humans { userName } }").await;
        assert!(err.starts_with("bob may not view directory"), "{err}");
        assert_eq!(seen.lock().unwrap().len(), 1, "bob's attempt never reached awzy-context");
    }
}

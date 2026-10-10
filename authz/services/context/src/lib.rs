//! awzy-context: the enterprise world model (who, what, how) and the attribute change feed.
//!
//! So far: people and groups, provisioned by the client's IdP over SCIM ([`scim`]); service
//! accounts, registered by the client's admins through the client portal (`/v1/service-accounts`;
//! ADR 0033); the snapshot of all of them the PDPs load (`GET /v1/principals`; ADR 0031); and
//! resources and work items, written by connectors ([`resources`]; demo plan step 2), served to
//! PDPs by ID (ADR 0044) and classified in the background ([`classify`]; ADR 0047). The Kafka
//! change feed comes next.

pub mod classify;
pub mod resources;
pub mod scim;
pub mod store;

use std::sync::Arc;

use awzy_service_auth::{require, Allowed};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::from_fn_with_state,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use store::{ContextStore, ServiceAccountInput, StoreError};

#[derive(Deserialize)]
struct SnapshotParams {
    /// The version the PDP already has.
    #[serde(default)]
    known: Option<i64>,
}

/// People and groups as Cedar entities, unless the caller already has this version.
/// Callers authenticate with service tokens (ADR 0054; see `Access`).
async fn principals(
    State(store): State<ContextStore>,
    Query(p): Query<SnapshotParams>,
) -> Result<Json<Value>, StatusCode> {
    let fail = |e: sqlx::Error| {
        tracing::error!(error = %e, "principals snapshot failed");
        StatusCode::SERVICE_UNAVAILABLE
    };
    let current = store.version().await.map_err(fail)?;
    if p.known == Some(current) {
        return Ok(Json(json!({ "version": current, "unchanged": true })));
    }
    let (version, entities) = store.snapshot().await.map_err(fail)?;
    Ok(Json(json!({ "version": version, "entities": entities })))
}

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!(error = %e, "awzy-context request failed");
    (StatusCode::SERVICE_UNAVAILABLE, "storage error").into_response()
}

async fn list_service_accounts(State(store): State<ContextStore>) -> Response {
    match store.service_accounts().await {
        Ok(list) => Json(json!({ "service_accounts": list })).into_response(),
        Err(e) => internal(e),
    }
}

/// Registers or changes a service account. `x-awzy-actor` names who did it, as the client
/// portal authorized them.
async fn put_service_account(
    State(store): State<ContextStore>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(mut input): Json<ServiceAccountInput>,
) -> Response {
    input.id = id;
    let by = headers.get("x-awzy-actor").and_then(|v| v.to_str().ok()).unwrap_or("unknown");
    match store.put_service_account(&input, by).await {
        Ok(sa) => {
            tracing::info!(service_account = %sa.id, owner = %sa.owner, ai = sa.ai, by, "service account registered");
            Json(json!({ "service_account": sa })).into_response()
        }
        Err(StoreError::Invalid(m) | StoreError::Conflict(m)) => (StatusCode::BAD_REQUEST, m).into_response(),
        Err(StoreError::Db(e)) => internal(e),
    }
}

async fn delete_service_account(
    State(store): State<ContextStore>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let by = headers.get("x-awzy-actor").and_then(|v| v.to_str().ok()).unwrap_or("unknown");
    match store.delete_service_account(&id).await {
        Ok(true) => {
            tracing::info!(service_account = %id, by, "service account removed");
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (StatusCode::NOT_FOUND, "no such service account").into_response(),
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct PeopleParams {
    search: Option<String>,
    limit: Option<i64>,
}

async fn directory_people(State(store): State<ContextStore>, Query(p): Query<PeopleParams>) -> Response {
    let search = p.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
    match store.directory_people(search, p.limit.unwrap_or(200).clamp(1, 1000)).await {
        Ok(people) => Json(json!({ "people": people })).into_response(),
        Err(e) => internal(e),
    }
}

async fn directory_groups(State(store): State<ContextStore>) -> Response {
    match store.directory_groups().await {
        Ok(groups) => Json(json!({ "groups": groups })).into_response(),
        Err(e) => internal(e),
    }
}

async fn directory_summary(State(store): State<ContextStore>) -> Response {
    match store.directory_summary().await {
        Ok(s) => Json(s).into_response(),
        Err(e) => internal(e),
    }
}

/// Who may call which routes (ADR 0054). Each group's callers come from the chart, as
/// `namespace/service-account`; a group it leaves empty is closed.
pub struct Access {
    verifier: Arc<awzy_service_auth::Verifier>,
    /// Principals, resources, and work items: the PDP, connectors, the synthetic Job.
    readers: Allowed,
    /// Writes to resources and work items: connectors, the synthetic Job.
    writers: Allowed,
    /// The directory and the service account registry: the client portal, the synthetic Job.
    directory: Allowed,
}

impl Access {
    /// From `AWZY_CONTEXT_READERS`, `AWZY_CONTEXT_WRITERS`, and `AWZY_CONTEXT_DIRECTORY`.
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            verifier: Arc::new(awzy_service_auth::Verifier::from_env("awzy-context")?),
            readers: Allowed::from_env("AWZY_CONTEXT_READERS"),
            writers: Allowed::from_env("AWZY_CONTEXT_WRITERS"),
            directory: Allowed::from_env("AWZY_CONTEXT_DIRECTORY"),
        })
    }

    /// Everyone may call everything: for tests and local runs.
    pub fn off() -> Self {
        Self::with_verifier(awzy_service_auth::Verifier::off(), &[], &[], &[])
    }

    pub fn with_verifier(
        verifier: awzy_service_auth::Verifier,
        readers: &[&str],
        writers: &[&str],
        directory: &[&str],
    ) -> Self {
        Self {
            verifier: Arc::new(verifier),
            readers: Allowed::only(readers),
            writers: Allowed::only(writers),
            directory: Allowed::only(directory),
        }
    }

    fn guard(&self, allowed: &Allowed) -> awzy_service_auth::Guard {
        awzy_service_auth::Guard::new(self.verifier.clone(), allowed.clone())
    }
}

/// The PDPs' snapshot at `/v1/principals`, the directory for the client portal at
/// `/v1/directory/*`, the service account registry at `/v1/service-accounts` (for the client
/// portal, which authorizes changes), resources and work items at `/v1/resources` and
/// `/v1/work-items` (for connectors), and SCIM at `/scim/v2` behind `scim_token`. Every route but
/// SCIM's needs a service token its group allows (`access`).
pub fn router(store: ContextStore, scim_token: &str, access: &Access) -> Router {
    let guarded = |r: Router, allowed: &Allowed| r.route_layer(from_fn_with_state(access.guard(allowed), require));
    let readers = Router::new().route("/v1/principals", get(principals)).with_state(store.clone());
    let directory = Router::new()
        .route("/v1/directory/summary", get(directory_summary))
        .route("/v1/directory/people", get(directory_people))
        .route("/v1/directory/groups", get(directory_groups))
        .route("/v1/service-accounts", get(list_service_accounts))
        .route("/v1/service-accounts/{id}", axum::routing::put(put_service_account).delete(delete_service_account))
        .with_state(store.clone());
    guarded(readers, &access.readers)
        .merge(guarded(resources::read_router(store.clone()), &access.readers))
        .merge(guarded(resources::write_router(store.clone()), &access.writers))
        .merge(guarded(directory, &access.directory))
        .merge(scim::router(store, scim_token))
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    use super::*;

    const TOKEN: &str = "scim-test-token-0123456789abcdef0123";

    async fn send(app: &Router, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/scim+json")
            .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// With --config=local-stack: an IdP provisions people and groups the way Okta and Entra do,
    /// and the PDPs' snapshot follows.
    #[tokio::test]
    async fn idp_provisioning_reaches_the_snapshot() {
        let Some(store) = store::tests::test_store().await else { return };
        let app = router(store, TOKEN, &Access::off());

        let unauthenticated =
            app.clone().oneshot(Request::get("/scim/v2/Users").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

        // Okta looks the user up first, then creates them.
        let (_, found) = send(
            &app,
            "GET",
            "/scim/v2/Users?filter=userName%20eq%20%22alice%40acme.com%22&startIndex=1&count=100",
            None,
        )
        .await;
        assert_eq!(found["totalResults"], 0);
        let (status, alice) = send(&app, "POST", "/scim/v2/Users", Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"], "userName": "alice@acme.com",
            "name": { "givenName": "Alice", "familyName": "Ng" }, "emails": [{ "primary": true, "value": "alice@acme.com" }],
            "active": true, "password": "never-stored", "externalId": "00u1abc",
        }))).await;
        assert_eq!(status, StatusCode::CREATED);
        assert!(alice.get("password").is_none(), "passwords are never stored or returned");
        let alice_id = alice["id"].as_str().unwrap().to_string();
        let (_, bob) = send(&app, "POST", "/scim/v2/Users", Some(json!({ "userName": "bob@acme.com" }))).await;
        let bob_id = bob["id"].as_str().unwrap().to_string();
        let (status, _) = send(&app, "POST", "/scim/v2/Users", Some(json!({ "userName": "ALICE@acme.com" }))).await;
        assert_eq!(status, StatusCode::CONFLICT);

        // Groups: create with a member, add one Okta's way, nest one inside another.
        let (_, sec) = send(
            &app,
            "POST",
            "/scim/v2/Groups",
            Some(json!({ "displayName": "Security", "members": [{ "value": alice_id }] })),
        )
        .await;
        let sec_id = sec["id"].as_str().unwrap().to_string();
        let (_, staff) =
            send(&app, "POST", "/scim/v2/Groups", Some(json!({ "displayName": "staff", "members": [] }))).await;
        let staff_id = staff["id"].as_str().unwrap().to_string();
        send(
            &app,
            "PATCH",
            &format!("/scim/v2/Groups/{staff_id}"),
            Some(json!({
                "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
                "Operations": [{ "op": "add", "path": "members", "value": [{ "value": bob_id }, { "value": sec_id }] }],
            })),
        )
        .await;
        // Okta renames with an object value.
        let (_, renamed) = send(
            &app,
            "PATCH",
            &format!("/scim/v2/Groups/{sec_id}"),
            Some(json!({
                "Operations": [{ "op": "replace", "value": { "id": sec_id, "displayName": "security" } }],
            })),
        )
        .await;
        assert_eq!(renamed["displayName"], "security");

        let (_, snap) = send(&app, "GET", "/v1/principals", None).await;
        let version = snap["version"].as_i64().unwrap();
        let entity = |snap: &Value, id: &str| {
            snap["entities"].as_array().unwrap().iter().find(|e| e["uid"]["id"] == id).cloned().unwrap()
        };
        assert_eq!(entity(&snap, "alice@acme.com")["parents"], json!([{ "type": "Group", "id": "security" }]));
        assert_eq!(entity(&snap, "security")["parents"], json!([{ "type": "Group", "id": "staff" }]), "nested");
        let (_, same) = send(&app, "GET", &format!("/v1/principals?known={version}"), None).await;
        assert_eq!(same["unchanged"], true);

        // Entra removes a member by filter path and deactivates with "Replace" and "False".
        send(
            &app,
            "PATCH",
            &format!("/scim/v2/Groups/{staff_id}"),
            Some(json!({
                "Operations": [{ "op": "Remove", "path": format!("members[value eq \"{bob_id}\"]") }],
            })),
        )
        .await;
        let (status, deactivated) = send(
            &app,
            "PATCH",
            &format!("/scim/v2/Users/{alice_id}"),
            Some(json!({
                "Operations": [{ "op": "Replace", "path": "active", "value": "False" }],
            })),
        )
        .await;
        assert_eq!((status, deactivated["active"].clone()), (StatusCode::OK, json!(false)));
        assert_eq!(deactivated["name"]["givenName"], "Alice", "PATCH keeps the rest of the resource");

        let (_, snap) = send(&app, "GET", &format!("/v1/principals?known={version}"), None).await;
        assert!(snap["version"].as_i64().unwrap() > version);
        assert_eq!(
            entity(&snap, "alice@acme.com"),
            json!({ "uid": { "type": "Human", "id": "alice@acme.com" }, "attrs": { "active": false }, "parents": [] })
        );
        assert_eq!(entity(&snap, "bob@acme.com")["parents"], json!([]), "removed from staff");

        let (_, people) = send(&app, "GET", "/v1/directory/people?search=ali", None).await;
        assert_eq!(people["people"][0]["user_name"], "alice@acme.com");
        assert_eq!(people["people"][0]["groups"], json!(["security"]));
        let (_, groups) = send(&app, "GET", "/v1/directory/groups", None).await;
        let staff_row = groups["groups"].as_array().unwrap().iter().find(|g| g["name"] == "staff").unwrap().clone();
        assert_eq!((staff_row["people"].clone(), staff_row["groups"].clone()), (json!(0), json!(1)));
        let (_, summary) = send(&app, "GET", "/v1/directory/summary", None).await;
        assert_eq!((summary["people"].clone(), summary["active_people"].clone()), (json!(2), json!(1)));

        let (status, _) = send(&app, "DELETE", &format!("/scim/v2/Users/{bob_id}"), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = send(&app, "GET", &format!("/scim/v2/Users/{bob_id}"), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (_, groups) = send(&app, "GET", "/scim/v2/Groups?filter=displayName%20eq%20%22staff%22", None).await;
        assert_eq!(
            groups["Resources"][0]["members"].as_array().unwrap().len(),
            1,
            "only the nested security group is left"
        );
    }

    #[tokio::test]
    async fn every_route_but_scim_needs_a_service_token() {
        use axum::{body::Body, http::Request as HttpRequest};
        use tower::ServiceExt;

        // A store that never connects: a guarded route must refuse before touching it.
        let pool = sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://awzy@127.0.0.1:1/none").unwrap();
        // A verifier with no keys refuses every token.
        let verifier =
            awzy_service_auth::Verifier::with_fixed_keys("awzy-context", "https://issuer.test", Default::default());
        let access =
            Access::with_verifier(verifier, &["awzy-system/awzy"], &["awzy-system/awzy"], &["awzy-portal/awzy"]);
        let app = router(ContextStore::new(pool), TOKEN, &access);
        let call = |method: &str, path: &str, auth: Option<String>| {
            let mut req = HttpRequest::builder().method(method).uri(path);
            if let Some(a) = auth {
                req = req.header("authorization", a);
            }
            app.clone().oneshot(req.body(Body::empty()).unwrap())
        };
        for (method, path) in [
            ("GET", "/v1/principals"),
            ("GET", "/v1/resources/synthetic:doc:1/entities"),
            ("POST", "/v1/resources/batch"),
            ("DELETE", "/v1/work-items/synthetic:ticket:1"),
            ("GET", "/v1/directory/summary"),
            ("PUT", "/v1/service-accounts/sales-genie"),
        ] {
            let status = call(method, path, None).await.unwrap().status();
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
            let forged = call(method, path, Some("Bearer not.a.token".into())).await.unwrap().status();
            assert_eq!(forged, StatusCode::UNAUTHORIZED, "{method} {path} with a bad token");
        }
        // SCIM answers to the IdP's token, not a service token: past auth, it reaches the store.
        let scim = call("GET", "/scim/v2/Users", Some(format!("Bearer {TOKEN}"))).await.unwrap().status();
        assert_ne!(scim, StatusCode::UNAUTHORIZED);
    }
}

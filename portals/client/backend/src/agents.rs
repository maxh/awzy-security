//! The API an app registers and retires its own agents through (ADR 0110), signed in with a token
//! it signs for itself (`sub` is its name): `GET /api/v1/agents`, and `PUT` and `DELETE
//! /api/v1/agents/{id}` for IDs under its prefix only. Each change is recorded in awzy-context with
//! the app as the actor.

use awzy_authz_proto::authz::v1::{authorizer_client::AuthorizerClient, ListTrustedIssuersRequest};
use awzy_service_auth::AuthChannel;
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{access::Auth, directory::ContextClient};

#[derive(Clone)]
struct AgentsState {
    auth: Auth,
    context: Option<ContextClient>,
}

/// An agent as the app registers it: like a service account in the client portal, and AI unless
/// it says otherwise.
#[derive(Deserialize)]
struct AgentInput {
    /// The accountable person's userName, provisioned by the IdP.
    owner: String,
    #[serde(default = "yes")]
    ai: bool,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default)]
    description: String,
}

fn yes() -> bool {
    true
}

#[derive(Serialize)]
struct ServiceAccountBody<'a> {
    id: &'a str,
    owner: &'a str,
    ai: bool,
    groups: &'a [String],
    description: &'a str,
}

fn plain(status: StatusCode, message: impl Into<String>) -> Response {
    (status, message.into()).into_response()
}

/// The app whose own token this is, or 401.
async fn signed_in_app(state: &AgentsState, headers: &HeaderMap) -> Result<String, Response> {
    let authorization = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    state.auth.app(authorization).await.map_err(|e| plain(StatusCode::UNAUTHORIZED, e.to_string()))
}

fn context(state: &AgentsState) -> Result<&ContextClient, Response> {
    state.context.as_ref().ok_or_else(|| plain(StatusCode::SERVICE_UNAVAILABLE, "awzy-context is not configured"))
}

/// Its agents' IDs start with its name and a dot.
fn prefix(app: &str) -> String {
    format!("{app}.")
}

fn under_prefix(app: &str, id: &str) -> Result<(), Response> {
    match id.strip_prefix(&prefix(app)) {
        Some(rest) if !rest.is_empty() => Ok(()),
        _ => Err(plain(StatusCode::FORBIDDEN, format!("{app} registers only agents under {:?}", prefix(app)))),
    }
}

fn unavailable(e: anyhow::Error) -> Response {
    tracing::error!(error = %e, "awzy-context call failed");
    plain(StatusCode::SERVICE_UNAVAILABLE, "awzy-context is unavailable")
}

async fn list(State(state): State<AgentsState>, headers: HeaderMap) -> Response {
    let app = match signed_in_app(&state, &headers).await {
        Ok(app) => app,
        Err(res) => return res,
    };
    let context = match context(&state) {
        Ok(c) => c,
        Err(res) => return res,
    };
    #[derive(Deserialize)]
    struct Wire {
        service_accounts: Vec<Value>,
    }
    match context.get::<Wire>("/v1/service-accounts").await {
        Ok(w) => {
            let mine: Vec<Value> = w
                .service_accounts
                .into_iter()
                .filter(|sa| sa["id"].as_str().is_some_and(|id| id.starts_with(&prefix(&app))))
                .collect();
            Json(json!({ "agents": mine })).into_response()
        }
        Err(e) => unavailable(e),
    }
}

async fn put_one(
    State(state): State<AgentsState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<AgentInput>,
) -> Response {
    let app = match signed_in_app(&state, &headers).await {
        Ok(app) => app,
        Err(res) => return res,
    };
    if let Err(res) = under_prefix(&app, &id) {
        return res;
    }
    let context = match context(&state) {
        Ok(c) => c,
        Err(res) => return res,
    };
    let body = ServiceAccountBody {
        id: &id,
        owner: &input.owner,
        ai: input.ai,
        groups: &input.groups,
        description: &input.description,
    };
    let Ok(body) = serde_json::to_value(&body) else {
        return plain(StatusCode::INTERNAL_SERVER_ERROR, "couldn't encode the agent");
    };
    let actor = awzy_model::Actor::app(&app).to_string();
    match context.call("PUT", &format!("/v1/service-accounts/{}", awzy_http::encode(&id)), Some(&actor), Some(body)).await
    {
        Ok((200, res)) => match serde_json::from_slice::<Value>(&res) {
            Ok(v) => Json(json!({ "agent": v["service_account"] })).into_response(),
            Err(_) => plain(StatusCode::BAD_GATEWAY, "awzy-context answered something unreadable"),
        },
        Ok((400, res)) => plain(StatusCode::BAD_REQUEST, String::from_utf8_lossy(&res)),
        Ok((status, res)) => {
            tracing::error!(status, body = %String::from_utf8_lossy(&res), "awzy-context refused an agent");
            plain(StatusCode::SERVICE_UNAVAILABLE, "awzy-context is unavailable")
        }
        Err(e) => unavailable(e),
    }
}

async fn delete_one(State(state): State<AgentsState>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    let app = match signed_in_app(&state, &headers).await {
        Ok(app) => app,
        Err(res) => return res,
    };
    if let Err(res) = under_prefix(&app, &id) {
        return res;
    }
    let context = match context(&state) {
        Ok(c) => c,
        Err(res) => return res,
    };
    let actor = awzy_model::Actor::app(&app).to_string();
    match context.call("DELETE", &format!("/v1/service-accounts/{}", awzy_http::encode(&id)), Some(&actor), None).await {
        Ok((204, _)) => StatusCode::NO_CONTENT.into_response(),
        Ok((404, _)) => plain(StatusCode::NOT_FOUND, format!("no agent {id:?}")),
        Ok((status, res)) => {
            tracing::error!(status, body = %String::from_utf8_lossy(&res), "awzy-context didn't retire an agent");
            plain(StatusCode::SERVICE_UNAVAILABLE, "awzy-context is unavailable")
        }
        Err(e) => unavailable(e),
    }
}

/// The routes, signed in by `auth`'s trusted apps.
pub fn router(auth: Auth, context: Option<ContextClient>) -> Router {
    Router::new()
        .route("/api/v1/agents", get(list))
        .route("/api/v1/agents/{id}", axum::routing::put(put_one).delete(delete_one))
        .with_state(AgentsState { auth, context })
}

/// Keeps each of `auths`' trusted apps current from the PDP (ADR 0110), forever.
pub async fn follow_trusted_issuers(pdp: AuthorizerClient<AuthChannel>, auths: &[&Auth]) {
    let lists = auths.iter().map(|a| a.trusted_issuers()).collect();
    awzy_identity::TrustedIssuers::follow(lists, move || {
        let mut pdp = pdp.clone();
        async move {
            let res = pdp.list_trusted_issuers(ListTrustedIssuersRequest {}).await?;
            Ok(res.into_inner().issuers.into_iter().map(Into::into).collect())
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn an_app_registers_only_agents_under_its_name() {
        assert!(under_prefix("acme-crm", "acme-crm.renewals-bot").is_ok());
        assert!(under_prefix("acme-crm", "acme-crm.").is_err());
        assert!(under_prefix("acme-crm", "acme-crm-2.bot").is_err());
        assert!(under_prefix("acme-crm", "sales-genie").is_err());
    }

    #[tokio::test]
    async fn a_token_not_signed_by_a_trusted_app_registers_nothing() {
        std::env::set_var("AWZY_TEST_AGENTS_SECRET", "test-secret");
        let token = awzy_identity::TokenConfig {
            issuer: "https://idp.test".into(),
            audience: "awzy".into(),
            jwks_ca_file: None,
            subject_claim: None,
            key: awzy_identity::TokenKey::Hs256SecretEnv("AWZY_TEST_AGENTS_SECRET".into()),
        };
        let auth = Auth::new(token, vec![], "portal".into(), None).unwrap();
        let alice = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &json!({ "iss": "https://idp.test", "aud": "awzy", "sub": "acme-crm", "exp": 4_000_000_000_u64 }),
            &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
        )
        .unwrap();
        let res = router(auth, None)
            .oneshot(
                Request::put("/api/v1/agents/acme-crm.bot")
                    .header("authorization", format!("Bearer {alice}"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"owner":"alice"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "the IdP's token, even one naming the app, isn't the app's");
    }
}

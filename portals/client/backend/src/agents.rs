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

/// Why a request did nothing, answered as its status and a sentence.
type Refused = (StatusCode, String);

fn refused(status: StatusCode, message: impl Into<String>) -> Refused {
    (status, message.into())
}

/// The app whose own token this is, or 401.
async fn signed_in_app(state: &AgentsState, headers: &HeaderMap) -> Result<String, Refused> {
    let authorization = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    state.auth.app(authorization).await.map_err(|e| refused(StatusCode::UNAUTHORIZED, e.to_string()))
}

fn context(state: &AgentsState) -> Result<&ContextClient, Refused> {
    state.context.as_ref().ok_or_else(|| refused(StatusCode::SERVICE_UNAVAILABLE, "awzy-context is not configured"))
}

/// Its agents' IDs start with its name and a dot.
fn prefix(app: &str) -> String {
    format!("{app}.")
}

fn under_prefix(app: &str, id: &str) -> Result<(), Refused> {
    match id.strip_prefix(&prefix(app)) {
        Some(rest) if !rest.is_empty() => Ok(()),
        _ => Err(refused(StatusCode::FORBIDDEN, format!("{app} registers only agents under {:?}", prefix(app)))),
    }
}

fn unavailable(e: impl std::fmt::Display) -> Refused {
    tracing::error!(error = %e, "awzy-context call failed");
    refused(StatusCode::SERVICE_UNAVAILABLE, "awzy-context is unavailable")
}

/// A service account as awzy-context sends it, passed on as it is.
#[derive(Deserialize, Serialize)]
struct Agent {
    id: String,
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

async fn list(State(state): State<AgentsState>, headers: HeaderMap) -> Result<Response, Refused> {
    let app = signed_in_app(&state, &headers).await?;
    #[derive(Deserialize)]
    struct Wire {
        service_accounts: Vec<Agent>,
    }
    let w: Wire = context(&state)?.get("/v1/service-accounts").await.map_err(unavailable)?;
    let mine: Vec<Agent> = w.service_accounts.into_iter().filter(|sa| sa.id.starts_with(&prefix(&app))).collect();
    Ok(Json(json!({ "agents": mine })).into_response())
}

async fn put_one(
    State(state): State<AgentsState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<AgentInput>,
) -> Result<Response, Refused> {
    let app = signed_in_app(&state, &headers).await?;
    under_prefix(&app, &id)?;
    let body = ServiceAccountBody {
        id: &id,
        owner: &input.owner,
        ai: input.ai,
        groups: &input.groups,
        description: &input.description,
    };
    let body = serde_json::to_value(&body).map_err(|e| refused(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let actor = awzy_model::Actor::app(&app).to_string();
    let path = format!("/v1/service-accounts/{}", awzy_http::encode(&id));
    match context(&state)?.call("PUT", &path, Some(&actor), Some(body)).await.map_err(unavailable)? {
        (200, res) => {
            #[derive(Deserialize)]
            struct Wire {
                service_account: Agent,
            }
            let w: Wire = serde_json::from_slice(&res).map_err(unavailable)?;
            Ok(Json(json!({ "agent": w.service_account })).into_response())
        }
        (400, res) => Err(refused(StatusCode::BAD_REQUEST, String::from_utf8_lossy(&res))),
        (status, res) => Err(unavailable(format!("{status}: {}", String::from_utf8_lossy(&res)))),
    }
}

async fn delete_one(
    State(state): State<AgentsState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Refused> {
    let app = signed_in_app(&state, &headers).await?;
    under_prefix(&app, &id)?;
    let actor = awzy_model::Actor::app(&app).to_string();
    let path = format!("/v1/service-accounts/{}", awzy_http::encode(&id));
    match context(&state)?.call("DELETE", &path, Some(&actor), None).await.map_err(unavailable)? {
        (204, _) => Ok(StatusCode::NO_CONTENT.into_response()),
        (404, _) => Err(refused(StatusCode::NOT_FOUND, format!("no agent {id:?}"))),
        (status, res) => Err(unavailable(format!("{status}: {}", String::from_utf8_lossy(&res)))),
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

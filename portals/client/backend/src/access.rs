//! Who may use the client portal and its MCP server (ADR 0020).
//!
//! - Every `/graphql` POST and every `/mcp` request needs a bearer token from the client's IdP,
//!   validated by awzy-identity the same way awzy-gateway does it. An agent acting for a person
//!   presents a token-exchange token (`act` claim) and is checked as that service account
//!   acting for them.
//! - Each operation is then checked with awzy-pdp as `view` or `administer` on a `PortalArea`.
//! - Verdicts are always applied, even when the install is in monitor mode. Monitor mode is for
//!   the client's own resources and must not open the portal that controls enforcement.
//! - A challenge is refused until the portal has pages to answer one (ADR 0054). If the PDP
//!   can't be reached, the portal refuses: it fails closed.

use std::sync::Arc;

use async_graphql::{Context, Enum};
use awzy_authz_proto::{
    authz::v1::{CheckRequest, EntityRef as WireRef, Verdict},
    entity_from_json, json_to_struct,
};
use awzy_identity::{Accessor, ServiceAccountConfig, TokenConfig, TokenKey, Verifier};
use awzy_model::entity_type;
use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;

use crate::Services;

/// The signed-in caller of one request.
#[derive(Clone, Debug)]
pub struct Viewer(pub Arc<Accessor>);

impl std::ops::Deref for Viewer {
    type Target = Accessor;

    fn deref(&self) -> &Accessor {
        &self.0
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum PortalAction {
    /// Read an area.
    View,
    /// Change what an area controls: publish policy, switch enforcement.
    Administer,
}

impl PortalAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Administer => "administer",
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum PortalArea {
    Decisions,
    Policies,
    Enforcement,
    /// People and groups from the IdP, and service accounts.
    Directory,
}

impl PortalArea {
    fn as_str(self) -> &'static str {
        match self {
            Self::Decisions => "decisions",
            Self::Policies => "policies",
            Self::Enforcement => "enforcement",
            Self::Directory => "directory",
        }
    }
}

/// Which surface the request came through, passed to Cedar as `context.client`.
#[derive(Copy, Clone, Debug)]
pub enum Surface {
    Portal,
    Mcp,
    /// Awzy Bot's tools, acting for the signed-in person (ADR 0048).
    AwzyBot,
}

#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    #[error("{actor} may not {action} {area} (decision {decision})")]
    Denied { actor: String, action: &'static str, area: &'static str, decision: String },
    #[error("{actor} may {action} {area} only after a {challenges} challenge, which the portal can't run yet (decision {decision})")]
    Challenged { actor: String, action: &'static str, area: &'static str, challenges: String, decision: String },
    #[error("the PDP is not configured (AWZY_PDP_ADDR)")]
    NotConfigured,
    #[error("awzy couldn't reach its policy decision point, so nothing was done: {0}")]
    Unavailable(String),
}

/// Checks one operation with the PDP and applies the verdict whatever the install's mode.
pub async fn authorize(
    services: &Services,
    viewer: &Viewer,
    surface: Surface,
    action: PortalAction,
    area: PortalArea,
) -> Result<(), AccessError> {
    let mut pdp = services.pdp.clone().ok_or(AccessError::NotConfigured)?;
    let wire = |r: &awzy_model::EntityRef| WireRef { r#type: r.kind.clone(), id: r.id.clone() };
    let mut entities: Vec<_> = viewer.entities.iter().map(entity_from_json).collect();
    entities.push(entity_from_json(&json!({
        "uid": { "type": entity_type::PORTAL_AREA, "id": area.as_str() },
        "attrs": { "sensitivity": "confidential" },
    })));
    let client = match surface {
        Surface::Portal => "portal",
        Surface::Mcp => "mcp",
        Surface::AwzyBot => "awzy-bot",
    };
    let req = CheckRequest {
        subject: Some(wire(&viewer.subject)),
        action: action.as_str().into(),
        resource: Some(WireRef { r#type: entity_type::PORTAL_AREA.into(), id: area.as_str().into() }),
        context: Some(json_to_struct(json!({ "client": client }).as_object().expect("object").clone())),
        on_behalf_of: viewer.on_behalf_of.as_ref().map(wire),
        entities,
        human_present: viewer.human_present,
        always_enforced: true,
    };
    let res = pdp.check(req).await.map_err(|s| AccessError::Unavailable(s.message().to_string()))?.into_inner();
    let (actor, action, area, decision) = (viewer.actor(), action.as_str(), area.as_str(), res.decision_id.clone());
    match res.verdict() {
        Verdict::Allow => Ok(()),
        Verdict::Challenge => {
            let challenges = res
                .challenges()
                .map(|c| c.as_str_name().trim_start_matches("CHALLENGE_KIND_").to_lowercase())
                .collect::<Vec<_>>();
            Err(AccessError::Challenged { actor, action, area, challenges: challenges.join(" and "), decision })
        }
        _ => Err(AccessError::Denied { actor, action, area, decision }),
    }
}

/// For GraphQL resolvers: the signed-in viewer, if they may do `action` in `area`.
pub(crate) async fn require(
    ctx: &Context<'_>,
    action: PortalAction,
    area: PortalArea,
) -> async_graphql::Result<Viewer> {
    let viewer = ctx.data::<Viewer>().map_err(|_| "sign in first")?.clone();
    let services = ctx.data::<Services>()?;
    authorize(services, &viewer, Surface::Portal, action, area).await?;
    Ok(viewer)
}

/// Validates bearer tokens for the portal.
#[derive(Clone)]
pub struct Auth(Arc<AuthInner>);

struct AuthInner {
    verifier: Verifier,
    service_accounts: Vec<ServiceAccountConfig>,
    /// The PDP takes service accounts from awzy-context instead (ADR 0033).
    defer_registry: bool,
    issuer: String,
    client_id: String,
    /// The portal's own URL, for MCP clients discovering how to sign in (RFC 9728).
    public_url: Option<String>,
}

impl Auth {
    pub fn new(
        token: TokenConfig,
        service_accounts: Vec<ServiceAccountConfig>,
        client_id: String,
        public_url: Option<String>,
    ) -> anyhow::Result<Self> {
        Ok(Self(Arc::new(AuthInner {
            verifier: Verifier::new(&token)?,
            defer_registry: false,
            service_accounts,
            issuer: token.issuer,
            client_id,
            public_url,
        })))
    }

    /// - AWZY_AUTH_ISSUER and AWZY_AUTH_JWKS_URL: the client's IdP. Required.
    /// - AWZY_AUTH_AUDIENCE: the `aud` tokens must carry (default `awzy`).
    /// - AWZY_AUTH_CA_FILE: PEM CA certificates for an IdP behind a private CA (optional).
    /// - AWZY_AUTH_SUBJECT_CLAIM: the claim that identifies a person (default `sub`).
    /// - AWZY_AUTH_CLIENT_ID: the portal frontend's OIDC client (default `awzy-client-portal`).
    /// - AWZY_SERVICE_ACCOUNTS_FILE: registered service accounts, a JSON list (optional); or
    ///   AWZY_SERVICE_ACCOUNT_REGISTRY=awzy-context to leave registration to awzy-context.
    /// - AWZY_PORTAL_PUBLIC_URL: the portal's URL as clients see it (optional).
    pub fn from_env() -> anyhow::Result<Self> {
        let var = |k: &str| {
            std::env::var(k).map_err(|_| anyhow::anyhow!("{k} is not set; the portal needs an identity provider"))
        };
        let token = TokenConfig {
            issuer: var("AWZY_AUTH_ISSUER")?,
            audience: std::env::var("AWZY_AUTH_AUDIENCE").unwrap_or_else(|_| "awzy".into()),
            jwks_ca_file: std::env::var("AWZY_AUTH_CA_FILE").ok(),
            subject_claim: std::env::var("AWZY_AUTH_SUBJECT_CLAIM").ok(),
            key: TokenKey::JwksUrl(var("AWZY_AUTH_JWKS_URL")?),
        };
        let service_accounts = match std::env::var("AWZY_SERVICE_ACCOUNTS_FILE") {
            Ok(path) => serde_json::from_slice(&std::fs::read(&path)?)?,
            Err(_) => vec![],
        };
        let client_id = std::env::var("AWZY_AUTH_CLIENT_ID").unwrap_or_else(|_| "awzy-client-portal".into());
        let mut auth = Self::new(token, service_accounts, client_id, std::env::var("AWZY_PORTAL_PUBLIC_URL").ok())?;
        if std::env::var("AWZY_SERVICE_ACCOUNT_REGISTRY").as_deref() == Ok("awzy-context") {
            Arc::get_mut(&mut auth.0).expect("not shared yet").defer_registry = true;
        }
        Ok(auth)
    }

    fn metadata_url(&self) -> Option<String> {
        self.0.public_url.as_ref().map(|u| format!("{}/.well-known/oauth-protected-resource", u.trim_end_matches('/')))
    }
}

/// Middleware: a valid bearer token, or 401. Puts the [`Viewer`] in the request's extensions.
pub async fn require_viewer(State(auth): State<Auth>, mut req: Request, next: Next) -> Response {
    let authorization = req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).map(str::to_string);
    let registry: &dyn awzy_identity::ServiceAccounts =
        if auth.0.defer_registry { &awzy_identity::DeferToPdp } else { &auth.0.service_accounts };
    match auth.0.verifier.accessor(authorization.as_deref(), registry).await {
        Ok(accessor) => {
            req.extensions_mut().insert(Viewer(Arc::new(accessor)));
            next.run(req).await
        }
        Err(e) => {
            let mut challenge = r#"Bearer error="invalid_token""#.to_string();
            if let Some(url) = auth.metadata_url() {
                challenge.push_str(&format!(r#", resource_metadata="{url}""#));
            }
            let mut res = (StatusCode::UNAUTHORIZED, e.to_string()).into_response();
            if let Ok(v) = HeaderValue::from_str(&challenge) {
                res.headers_mut().insert(header::WWW_AUTHENTICATE, v);
            }
            res
        }
    }
}

/// Unauthenticated: how to sign in. `/auth/config` for the portal frontend, and OAuth protected
/// resource metadata (RFC 9728) for MCP clients.
pub fn routes(auth: Auth) -> Router {
    let config = auth.clone();
    Router::new()
        .route(
            "/auth/config",
            get(move || async move { Json(json!({ "issuer": config.0.issuer, "clientId": config.0.client_id })) }),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(move || async move {
                Json(json!({
                    "resource": auth.0.public_url,
                    "authorization_servers": [auth.0.issuer],
                    "bearer_methods_supported": ["header"],
                }))
            }),
        )
}

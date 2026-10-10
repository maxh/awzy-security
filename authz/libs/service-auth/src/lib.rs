//! awzy-service-auth: how awzy's services inside an install prove who is calling (ADR 0055).
//!
//! A caller sends the projected Kubernetes service-account token the chart mounts for the
//! service it calls (`TokenSource`). The receiver checks it locally against the cluster's signing
//! keys, its own name as the audience, and the cluster's issuer (`Verifier`), then lets through
//! only the callers a route group allows (`require`). No call to the API server per request.
//!
//! A caller is a namespace and service account (`awzy-system/awzy`). Which callers belong to a
//! route group comes from the chart, which knows the install's namespaces
//! (`Allowed::from_env("AWZY_CONTEXT_WRITERS")`).
//!
//! Both sides are on in Kubernetes (every pod has `KUBERNETES_SERVICE_HOST`) and off elsewhere,
//! where there's no service-account token: a laptop, a test. `AWZY_SERVICE_AUTH=on|off`
//! overrides that.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use awzy_identity::jwks::Jwks;
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

/// Where Kubernetes mounts every pod's own service-account token and the cluster's CA.
const POD_TOKEN: &str = "/var/run/secrets/kubernetes.io/serviceaccount/token";
const CLUSTER_CA: &str = "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt";
/// The API server serves the keys that sign service-account tokens here, in every cluster.
const CLUSTER_JWKS: &str = "https://kubernetes.default.svc/openid/v1/jwks";
/// Where the chart mounts the projected tokens a workload sends, one file per audience.
pub const TOKEN_DIR: &str = "/var/run/secrets/awzy/tokens";

fn turned_off() -> bool {
    match std::env::var("AWZY_SERVICE_AUTH").as_deref() {
        Ok("off") => true,
        Ok("on") => false,
        _ => std::env::var_os("KUBERNETES_SERVICE_HOST").is_none(),
    }
}

/// The workload that called, from its token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    pub namespace: String,
    pub service_account: String,
}

impl std::fmt::Display for Caller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.namespace, self.service_account)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("no service token (Authorization: Bearer)")]
    Missing,
    #[error("invalid service token: {0}")]
    Invalid(String),
    #[error("{0} may not call this")]
    Forbidden(String),
}

/// Who may call a route group.
#[derive(Debug, Clone)]
pub enum Allowed {
    /// Any workload in the cluster with a valid token. For the PDP, which enforcement points
    /// anywhere call, and whose policies decide what they may do.
    AnyServiceAccount,
    /// These callers, as `namespace/service-account`.
    Only(Vec<String>),
}

impl Allowed {
    pub fn only(callers: &[&str]) -> Self {
        Self::Only(callers.iter().map(|c| c.to_string()).collect())
    }

    /// The callers in `var`, comma-separated `namespace/service-account`. Unset or empty means
    /// nobody, so a route group the chart forgot stays closed.
    pub fn from_env(var: &str) -> Self {
        let callers = std::env::var(var).unwrap_or_default();
        Self::Only(callers.split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect())
    }
}

enum Keys {
    Cluster(Jwks),
    Fixed(HashMap<String, DecodingKey>),
}

impl Keys {
    async fn key(&self, kid: &str) -> anyhow::Result<DecodingKey> {
        match self {
            Self::Cluster(jwks) => jwks.key(kid).await,
            Self::Fixed(keys) => keys.get(kid).cloned().ok_or_else(|| anyhow::anyhow!("unknown key {kid:?}")),
        }
    }
}

struct Checks {
    audience: String,
    issuer: String,
    keys: Keys,
}

/// Checks callers' tokens for one receiving service. Off, it lets everyone through.
pub struct Verifier(Option<Checks>);

impl Verifier {
    pub fn off() -> Self {
        Self(None)
    }

    /// The cluster's keys and issuer. Off outside Kubernetes, or with `AWZY_SERVICE_AUTH=off`.
    pub fn from_env(audience: &str) -> anyhow::Result<Self> {
        if turned_off() {
            tracing::warn!(
                audience,
                "service auth is off (not in Kubernetes, or AWZY_SERVICE_AUTH=off): callers are not authenticated"
            );
            return Ok(Self::off());
        }
        let own = std::fs::read_to_string(POD_TOKEN).map_err(|e| anyhow::anyhow!("reading {POD_TOKEN}: {e}"))?;
        let issuer = issuer_of(&own)?;
        let ca = std::fs::read(CLUSTER_CA).map_err(|e| anyhow::anyhow!("reading {CLUSTER_CA}: {e}"))?;
        let jwks = Jwks::new(CLUSTER_JWKS, Some(&ca))?.with_bearer_file(POD_TOKEN);
        tracing::info!(audience, %issuer, "checking service tokens");
        Ok(Self::with_keys(audience, &issuer, Keys::Cluster(jwks)))
    }

    /// Fixed keys, by key ID: for tests.
    pub fn with_fixed_keys(audience: &str, issuer: &str, keys: HashMap<String, DecodingKey>) -> Self {
        Self::with_keys(audience, issuer, Keys::Fixed(keys))
    }

    fn with_keys(audience: &str, issuer: &str, keys: Keys) -> Self {
        Self(Some(Checks { audience: audience.into(), issuer: issuer.into(), keys }))
    }

    /// Checks the request's `Authorization` header against `allowed`. `Ok(None)` when off.
    pub async fn check(&self, authorization: Option<&str>, allowed: &Allowed) -> Result<Option<Caller>, AuthError> {
        let Some(checks) = &self.0 else { return Ok(None) };
        let token = authorization.and_then(|h| h.strip_prefix("Bearer ")).ok_or(AuthError::Missing)?.trim();
        let caller = checks.verify(token).await?;
        match allowed {
            Allowed::AnyServiceAccount => Ok(Some(caller)),
            Allowed::Only(callers) => {
                if callers.contains(&caller.to_string()) {
                    Ok(Some(caller))
                } else {
                    Err(AuthError::Forbidden(caller.to_string()))
                }
            }
        }
    }
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
}

impl Checks {
    async fn verify(&self, token: &str) -> Result<Caller, AuthError> {
        let invalid = |e: &dyn std::fmt::Display| AuthError::Invalid(e.to_string());
        let header = decode_header(token).map_err(|e| invalid(&e))?;
        // Kubernetes signs with RS256 or ES256; anything else (none, HS*) is refused outright.
        if !matches!(header.alg, Algorithm::RS256 | Algorithm::ES256) {
            return Err(AuthError::Invalid(format!("unexpected algorithm {:?}", header.alg)));
        }
        let kid = header.kid.ok_or_else(|| AuthError::Invalid("no key ID".into()))?;
        let key = self.keys.key(&kid).await.map_err(|e| invalid(&e))?;
        let mut validation = Validation::new(header.alg);
        validation.set_audience(&[&self.audience]);
        validation.set_issuer(&[&self.issuer]);
        validation.set_required_spec_claims(&["exp", "aud", "iss", "sub"]);
        let claims = decode::<Claims>(token, &key, &validation).map_err(|e| invalid(&e))?.claims;
        // system:serviceaccount:<namespace>:<name>
        match claims.sub.split(':').collect::<Vec<_>>().as_slice() {
            ["system", "serviceaccount", namespace, name] => {
                Ok(Caller { namespace: namespace.to_string(), service_account: name.to_string() })
            }
            _ => Err(AuthError::Invalid(format!("{} isn't a service account", claims.sub))),
        }
    }
}

/// The issuer (`iss`) in a token, read without verifying it: the pod's own token, to learn which
/// issuer the cluster's other tokens must carry.
fn issuer_of(token: &str) -> anyhow::Result<String> {
    let payload = token.trim().split('.').nth(1).ok_or_else(|| anyhow::anyhow!("not a JWT"))?;
    let claims: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload)?)?;
    claims["iss"].as_str().map(str::to_string).ok_or_else(|| anyhow::anyhow!("the pod's token has no issuer"))
}

/// What `require` needs: the verifier and who may call the routes it guards.
#[derive(Clone)]
pub struct Guard {
    verifier: Arc<Verifier>,
    allowed: Allowed,
}

impl Guard {
    pub fn new(verifier: Arc<Verifier>, allowed: Allowed) -> Self {
        Self { verifier, allowed }
    }
}

/// axum middleware: 401 without a valid service token, 403 for a caller the routes don't allow.
///
/// `router.route_layer(axum::middleware::from_fn_with_state(Guard::new(verifier, allowed), require))`
pub async fn require(State(guard): State<Guard>, req: Request, next: Next) -> Response {
    let authorization = req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    match guard.verifier.check(authorization, &guard.allowed).await {
        Ok(_) => next.run(req).await,
        Err(e @ AuthError::Forbidden(_)) => {
            tracing::warn!(error = %e, path = %req.uri().path(), "refused a service call");
            (StatusCode::FORBIDDEN, e.to_string()).into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, path = %req.uri().path(), "unauthenticated service call");
            (StatusCode::UNAUTHORIZED, e.to_string()).into_response()
        }
    }
}

/// The tokens a workload sends, one per audience, as the chart mounts them.
pub struct TokenSource(Option<PathBuf>);

impl TokenSource {
    pub fn off() -> Self {
        Self(None)
    }

    /// `AWZY_SERVICE_TOKEN_DIR` (default `TOKEN_DIR`); off outside Kubernetes, or with
    /// `AWZY_SERVICE_AUTH=off`.
    pub fn from_env() -> Self {
        if turned_off() {
            return Self::off();
        }
        Self(Some(std::env::var("AWZY_SERVICE_TOKEN_DIR").map(PathBuf::from).unwrap_or_else(|_| TOKEN_DIR.into())))
    }

    pub fn in_dir(dir: impl Into<PathBuf>) -> Self {
        Self(Some(dir.into()))
    }

    /// The `Authorization` header value for calling `audience`, read fresh: the kubelet rotates
    /// the file. `None` when off.
    pub fn authorization(&self, audience: &str) -> anyhow::Result<Option<String>> {
        let Some(dir) = &self.0 else { return Ok(None) };
        let path = dir.join(audience);
        let token = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("reading the {audience} token in {}: {e}", path.display()))?;
        Ok(Some(format!("Bearer {}", token.trim())))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::{body::Body, http::Request as HttpRequest, middleware::from_fn_with_state, routing::get, Router};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use p256::{
        ecdsa::SigningKey,
        pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding},
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use super::*;

    const ISSUER: &str = "https://oidc.cluster.test";

    struct Cluster {
        signing: EncodingKey,
        verifier: Arc<Verifier>,
    }

    fn cluster(audience: &str) -> Cluster {
        cluster_with_key(audience, 7)
    }

    /// A cluster whose tokens are signed by the key made from `seed`, and verified as k1.
    fn cluster_with_key(audience: &str, seed: u8) -> Cluster {
        let key = SigningKey::from_bytes(&[seed; 32].into()).unwrap();
        let signing = EncodingKey::from_ec_pem(key.to_pkcs8_pem(LineEnding::LF).unwrap().as_bytes()).unwrap();
        let public = key.verifying_key().to_public_key_pem(LineEnding::LF).unwrap();
        let keys = [("k1".to_string(), DecodingKey::from_ec_pem(public.as_bytes()).unwrap())].into();
        let verifier = Verifier::with_fixed_keys(audience, ISSUER, keys);
        Cluster { signing, verifier: Arc::new(verifier) }
    }

    fn now() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    fn token(c: &Cluster, sub: &str, overrides: Value) -> String {
        let mut claims = json!({ "iss": ISSUER, "aud": ["awzy-context"], "sub": sub, "exp": now() + 600 });
        for (k, v) in overrides.as_object().unwrap() {
            claims[k] = v.clone();
        }
        let header = Header { kid: Some("k1".into()), ..Header::new(Algorithm::ES256) };
        format!("Bearer {}", encode(&header, &claims, &c.signing).unwrap())
    }

    const PDP: &str = "system:serviceaccount:awzy-system:awzy-pdp";

    #[tokio::test]
    async fn an_allowed_service_account_in_the_install_passes() {
        let c = cluster("awzy-context");
        let caller =
            c.verifier.check(Some(&token(&c, PDP, json!({}))), &Allowed::only(&["awzy-system/awzy-pdp"])).await;
        assert_eq!(
            caller.unwrap(),
            Some(Caller { namespace: "awzy-system".into(), service_account: "awzy-pdp".into() })
        );
    }

    #[tokio::test]
    async fn tokens_for_another_service_or_cluster_or_expired_are_refused() {
        let c = cluster("awzy-context");
        let only_pdp = Allowed::only(&["awzy-system/awzy-pdp"]);
        for (why, overrides) in [
            ("for awzy-policy", json!({ "aud": ["awzy-policy"] })),
            ("from another cluster", json!({ "iss": "https://elsewhere.test" })),
            ("expired", json!({ "exp": now() - 3600 })),
        ] {
            let res = c.verifier.check(Some(&token(&c, PDP, overrides)), &only_pdp).await;
            assert!(matches!(res, Err(AuthError::Invalid(_))), "{why}: {res:?}");
        }
        let forged = token(&cluster_with_key("awzy-context", 9), PDP, json!({}));
        let res = c.verifier.check(Some(&forged), &only_pdp).await;
        assert!(matches!(res, Err(AuthError::Invalid(_))), "signed by another key: {res:?}");
        assert_eq!(c.verifier.check(None, &only_pdp).await, Err(AuthError::Missing));
    }

    #[tokio::test]
    async fn the_same_name_in_another_namespace_is_not_the_pdp() {
        let c = cluster("awzy-context");
        let imposter = token(&c, "system:serviceaccount:some-app:awzy-pdp", json!({}));
        assert_eq!(
            c.verifier.check(Some(&imposter), &Allowed::only(&["awzy-system/awzy-pdp"])).await,
            Err(AuthError::Forbidden("some-app/awzy-pdp".into()))
        );
        // The PDP itself accepts any workload in the cluster; its policies decide the rest.
        assert!(c.verifier.check(Some(&imposter), &Allowed::AnyServiceAccount).await.is_ok());
    }

    #[tokio::test]
    async fn unsigned_and_shared_secret_tokens_are_refused() {
        let c = cluster("awzy-context");
        let claims = json!({ "iss": ISSUER, "aud": ["awzy-context"], "sub": PDP, "exp": now() + 600 });
        let hs = encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(b"guess")).unwrap();
        let res = c.verifier.check(Some(&format!("Bearer {hs}")), &Allowed::AnyServiceAccount).await;
        assert!(matches!(res, Err(AuthError::Invalid(ref m)) if m.contains("algorithm")), "{res:?}");
    }

    #[tokio::test]
    async fn the_middleware_answers_401_403_or_passes() {
        let c = cluster("awzy-context");
        let app = Router::new().route("/v1/principals", get(|| async { "ok" })).route_layer(from_fn_with_state(
            Guard::new(c.verifier.clone(), Allowed::only(&["awzy-system/awzy-pdp"])),
            require,
        ));
        let call = |auth: Option<String>| {
            let app = app.clone();
            async move {
                let mut req = HttpRequest::get("/v1/principals");
                if let Some(a) = auth {
                    req = req.header(header::AUTHORIZATION, a);
                }
                app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap().status()
            }
        };
        assert_eq!(call(None).await, StatusCode::UNAUTHORIZED);
        assert_eq!(
            call(Some(token(&c, "system:serviceaccount:awzy-system:awzy-gateway", json!({})))).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(call(Some(token(&c, PDP, json!({})))).await, StatusCode::OK);

        let off = Router::new()
            .route("/", get(|| async { "ok" }))
            .route_layer(from_fn_with_state(Guard::new(Arc::new(Verifier::off()), Allowed::only(&[])), require));
        let res = off.oneshot(HttpRequest::get("/").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "off lets everyone through");
    }

    #[test]
    fn route_groups_come_from_the_chart_and_default_to_nobody() {
        std::env::set_var("AWZY_TEST_SERVICE_AUTH_GROUP", " awzy-system/awzy, awzy-portal/awzy ,");
        let Allowed::Only(callers) = Allowed::from_env("AWZY_TEST_SERVICE_AUTH_GROUP") else { panic!() };
        assert_eq!(callers, ["awzy-system/awzy", "awzy-portal/awzy"]);
        let Allowed::Only(none) = Allowed::from_env("AWZY_TEST_SERVICE_AUTH_UNSET") else { panic!() };
        assert!(none.is_empty(), "a group the chart didn't fill is closed");
    }

    #[test]
    fn the_issuer_comes_from_the_pods_own_token() {
        let c = cluster("awzy-context");
        let own = token(&c, PDP, json!({})).replace("Bearer ", "");
        assert_eq!(issuer_of(&own).unwrap(), ISSUER);
        assert!(issuer_of("not-a-jwt").is_err());
    }

    #[test]
    fn tokens_are_read_fresh_per_audience() {
        let dir = std::env::temp_dir().join(format!("awzy-service-auth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = TokenSource::in_dir(&dir);
        std::fs::write(dir.join("awzy-context"), "first\n").unwrap();
        assert_eq!(source.authorization("awzy-context").unwrap().as_deref(), Some("Bearer first"));
        std::fs::write(dir.join("awzy-context"), "rotated").unwrap();
        assert_eq!(source.authorization("awzy-context").unwrap().as_deref(), Some("Bearer rotated"));
        assert!(source.authorization("awzy-policy").is_err(), "no token mounted for that audience");
        assert_eq!(TokenSource::off().authorization("awzy-context").unwrap(), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

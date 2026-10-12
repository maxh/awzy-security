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

mod refused;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

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
    Cluster(Box<Jwks>),
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
    verified: Verified,
}

/// Tokens that passed, by the exact token, with their caller and expiry. A workload sends the same
/// projected token for minutes (the kubelet renews it at 80% of its life), so checking its
/// signature again on every request costs CPU and proves nothing new: the PDP's hottest path.
#[derive(Default)]
struct Verified(RwLock<HashMap<String, (Caller, u64)>>);

impl Verified {
    /// Enough for every workload in an install, a few tokens each.
    const MAX: usize = 10_000;

    fn get(&self, token: &str, now: u64) -> Option<Caller> {
        let cache = self.0.read().unwrap_or_else(|e| e.into_inner());
        cache.get(token).filter(|(_, exp)| *exp > now).map(|(caller, _)| caller.clone())
    }

    fn insert(&self, token: &str, caller: &Caller, exp: u64, now: u64) {
        let mut cache = self.0.write().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= Self::MAX {
            cache.retain(|_, (_, exp)| *exp > now);
            if cache.len() >= Self::MAX {
                cache.clear();
            }
        }
        cache.insert(token.to_string(), (caller.clone(), exp));
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
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
        Ok(Self::with_keys(audience, &issuer, Keys::Cluster(Box::new(jwks))))
    }

    /// Fixed keys, by key ID: for tests.
    pub fn with_fixed_keys(audience: &str, issuer: &str, keys: HashMap<String, DecodingKey>) -> Self {
        Self::with_keys(audience, issuer, Keys::Fixed(keys))
    }

    fn with_keys(audience: &str, issuer: &str, keys: Keys) -> Self {
        Self(Some(Checks { audience: audience.into(), issuer: issuer.into(), keys, verified: Verified::default() }))
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
    exp: u64,
}

impl Checks {
    /// The token's caller: from the cache until the token expires, verified otherwise. Each
    /// verifier checks one audience, so a token cached here passed for this service.
    async fn verify(&self, token: &str) -> Result<Caller, AuthError> {
        let now = unix_now();
        if let Some(caller) = self.verified.get(token, now) {
            return Ok(caller);
        }
        let (caller, exp) = self.verify_signed(token).await?;
        self.verified.insert(token, &caller, exp, now);
        Ok(caller)
    }

    async fn verify_signed(&self, token: &str) -> Result<(Caller, u64), AuthError> {
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
                Ok((Caller { namespace: namespace.to_string(), service_account: name.to_string() }, claims.exp))
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
        refused::register();
        Self { verifier, allowed }
    }
}

/// What a refusal is counted under: a gRPC method's path (a fixed set), or an HTTP route's
/// pattern, never a path with an ID in it.
fn route_of(req: &Request) -> String {
    let path = req.uri().path();
    if path.starts_with("/awzy.") {
        return path.to_string();
    }
    req.extensions()
        .get::<axum::extract::MatchedPath>()
        .map_or_else(|| "unmatched".to_string(), |m| m.as_str().to_string())
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
            let caller = match &e {
                AuthError::Forbidden(caller) => Some(caller.as_str()),
                _ => None,
            };
            refused::count(&route_of(&req), refused::Reason::NotAllowed, caller);
            (StatusCode::FORBIDDEN, e.to_string()).into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, path = %req.uri().path(), "unauthenticated service call");
            refused::count(&route_of(&req), refused::Reason::Unauthenticated, None);
            (StatusCode::UNAUTHORIZED, e.to_string()).into_response()
        }
    }
}

/// Where a guarded router sends each request path: the first rule whose path matches decides,
/// and a path no rule names is refused. For gRPC services, which tonic mounts as one route, so
/// per-method rules have to go by path (`/awzy.policy.v1.PolicyService/Publish`).
#[derive(Clone)]
pub struct PathGuard {
    verifier: Arc<Verifier>,
    rules: Arc<Vec<(String, Rule)>>,
}

#[derive(Clone, Debug)]
pub enum Rule {
    /// Callers this group allows.
    Allow(Allowed),
    /// No service token: the method authenticates its callers another way (signatures).
    Exempt,
}

impl PathGuard {
    pub fn new(verifier: Arc<Verifier>, rules: Vec<(String, Rule)>) -> Self {
        refused::register();
        Self { verifier, rules: Arc::new(rules) }
    }

    /// What's wrong with these rules for the gRPC services `proto` declares: a method no rule
    /// names (refused for every caller), a method named twice, or a rule for a method the service
    /// doesn't have. Empty when there's nothing; for each service's tests.
    pub fn gaps(&self, proto: &str) -> Vec<String> {
        let methods = proto_methods(proto);
        let mut gaps = Vec::new();
        for m in &methods {
            match self.rules.iter().filter(|(path, _)| path == m).count() {
                0 => gaps.push(format!("{m}: no rule names it, so every call is refused")),
                1 => {}
                n => gaps.push(format!("{m}: {n} rules name it")),
            }
        }
        for (path, _) in self.rules.iter() {
            if !methods.contains(path) {
                gaps.push(format!("{path}: a rule names it, but the proto has no such method"));
            }
        }
        gaps
    }
}

/// Each method's path, `/<package>.<Service>/<Method>`, for every service in `proto`.
fn proto_methods(proto: &str) -> Vec<String> {
    let package = proto
        .lines()
        .find_map(|l| l.trim().strip_prefix("package "))
        .map(|p| p.trim_end_matches(';').trim())
        .unwrap_or_default();
    let mut service = None;
    let mut methods = Vec::new();
    for line in proto.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix("service ") {
            service = name.split_whitespace().next().map(str::to_string);
        } else if let (Some(s), Some(rest)) = (&service, line.strip_prefix("rpc ")) {
            if let Some(method) = rest.split('(').next() {
                methods.push(format!("/{package}.{s}/{}", method.trim()));
            }
        }
    }
    methods
}

/// axum middleware for a `PathGuard`: the matching rule's callers, 401/403 otherwise.
pub async fn require_by_path(State(guard): State<PathGuard>, req: Request, next: Next) -> Response {
    let rule = guard.rules.iter().find(|(path, _)| path == req.uri().path()).map(|(_, r)| r.clone());
    match rule {
        Some(Rule::Exempt) => next.run(req).await,
        Some(Rule::Allow(allowed)) => require(State(Guard::new(guard.verifier.clone(), allowed)), req, next).await,
        None if guard.verifier.0.is_none() => next.run(req).await,
        None => {
            tracing::warn!(path = %req.uri().path(), "refused a call to a method no rule names");
            refused::count(req.uri().path(), refused::Reason::Unlisted, None);
            (StatusCode::FORBIDDEN, "no caller may call this").into_response()
        }
    }
}

/// A gRPC channel that sends the service token for `audience` with every call.
pub type AuthChannel = tonic::service::interceptor::InterceptedService<tonic::transport::Channel, TokenInterceptor>;

/// Wraps `channel` to send this workload's token for `audience` (`TokenSource::from_env`).
pub fn grpc_channel(channel: tonic::transport::Channel, audience: &str) -> AuthChannel {
    tonic::service::interceptor::InterceptedService::new(
        channel,
        TokenInterceptor { audience: audience.to_string(), tokens: Arc::new(TokenSource::from_env()) },
    )
}

/// Runs a gRPC call, and once more if it failed without the server's answer: the connection broke
/// under it, as when the pod it reached is shutting down (ADR 0072), and a second try gets a new
/// connection to another pod. Depending on where the connection broke, that's `Unavailable`,
/// `Cancelled` (closed before the request was sent), `Internal` (the stream reset), or `Unknown`.
/// Only for calls that are safe to repeat, like a PDP check.
pub async fn retry_once_if_unanswered<T, F, Fut>(mut call: F) -> Result<T, tonic::Status>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, tonic::Status>>,
{
    match call().await {
        Err(status) if unanswered(&status) => {
            tracing::debug!(%status, "retrying a call the server didn't answer");
            call().await
        }
        result => result,
    }
}

fn unanswered(status: &tonic::Status) -> bool {
    use tonic::Code;
    matches!(status.code(), Code::Unavailable | Code::Cancelled | Code::Internal | Code::Unknown)
}

/// Adds `authorization` metadata to each gRPC call, read from the mounted token.
#[derive(Clone)]
pub struct TokenInterceptor {
    audience: String,
    tokens: Arc<TokenSource>,
}

impl tonic::service::Interceptor for TokenInterceptor {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        let authorization = self
            .tokens
            .authorization(&self.audience)
            .map_err(|e| tonic::Status::unauthenticated(format!("no service token to send: {e}")))?;
        if let Some(a) = authorization {
            let value = a.parse().map_err(|_| tonic::Status::internal("the service token isn't a valid header"))?;
            req.metadata_mut().insert("authorization", value);
        }
        // The caller's trace goes along (ADR 0097); nothing when traces are off.
        for (key, value) in awzy_telemetry::trace_headers() {
            if let (Ok(key), Ok(value)) = (
                tonic::metadata::MetadataKey::from_bytes(key.as_bytes()),
                value.parse::<tonic::metadata::MetadataValue<_>>(),
            ) {
                req.metadata_mut().insert(key, value);
            }
        }
        Ok(req)
    }
}

/// The tokens a workload sends, one per audience, as the chart mounts them.
#[derive(Clone)]
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

    fn cached(c: &Cluster) -> usize {
        c.verifier.0.as_ref().unwrap().verified.0.read().unwrap().len()
    }

    #[tokio::test]
    async fn a_verified_token_is_remembered_until_it_expires_and_callers_are_still_checked() {
        let c = cluster("awzy-context");
        let pdp = token(&c, PDP, json!({}));
        let only_pdp = Allowed::only(&["awzy-system/awzy-pdp"]);
        for _ in 0..3 {
            assert!(c.verifier.check(Some(&pdp), &only_pdp).await.is_ok());
        }
        assert_eq!(cached(&c), 1, "verified once, then remembered");
        // Remembered or not, a caller the route doesn't allow is refused.
        assert_eq!(
            c.verifier.check(Some(&pdp), &Allowed::only(&["awzy-system/awzy-reporter"])).await,
            Err(AuthError::Forbidden("awzy-system/awzy-pdp".into()))
        );

        // Refused tokens aren't remembered.
        let elsewhere = token(&c, PDP, json!({ "aud": ["awzy-policy"] }));
        assert!(c.verifier.check(Some(&elsewhere), &only_pdp).await.is_err());
        assert_eq!(cached(&c), 1);

        // Past its expiry, a remembered token isn't taken from the cache.
        let checks = c.verifier.0.as_ref().unwrap();
        let bearer = pdp.strip_prefix("Bearer ").unwrap();
        assert!(checks.verified.get(bearer, now()).is_some());
        assert!(checks.verified.get(bearer, now() + 601).is_none());
    }

    #[test]
    fn the_cache_stays_bounded() {
        let verified = Verified::default();
        let caller = Caller { namespace: "awzy-system".into(), service_account: "awzy".into() };
        for i in 0..Verified::MAX + 10 {
            verified.insert(&format!("token-{i}"), &caller, 100, 50);
        }
        assert!(verified.0.read().unwrap().len() <= Verified::MAX);
        assert!(verified.get(&format!("token-{}", Verified::MAX + 9), 50).is_some(), "the newest is kept");
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
    fn gaps_name_methods_without_a_rule_and_rules_without_a_method() {
        let proto = "syntax = \"proto3\";\npackage awzy.demo.v1;\n\nservice Things {\n  rpc Get(GetRequest) returns (Thing);\n  // Writes.\n  rpc Put(PutRequest) returns (Thing);\n  rpc Delete(DeleteRequest) returns (Empty);\n}\n";
        let rule = |m: &str| (format!("/awzy.demo.v1.Things/{m}"), Rule::Allow(Allowed::AnyServiceAccount));
        let complete = PathGuard::new(Arc::new(Verifier::off()), vec![rule("Get"), rule("Put"), rule("Delete")]);
        assert!(complete.gaps(proto).is_empty());

        let guard = PathGuard::new(Arc::new(Verifier::off()), vec![rule("Get"), rule("Get"), rule("Remove")]);
        let gaps = guard.gaps(proto);
        assert!(gaps.iter().any(|g| g.starts_with("/awzy.demo.v1.Things/Put: no rule")), "{gaps:?}");
        assert!(gaps.iter().any(|g| g.starts_with("/awzy.demo.v1.Things/Delete: no rule")), "{gaps:?}");
        assert!(gaps.iter().any(|g| g.starts_with("/awzy.demo.v1.Things/Get: 2 rules")), "{gaps:?}");
        assert!(gaps.iter().any(|g| g.starts_with("/awzy.demo.v1.Things/Remove: a rule names it")), "{gaps:?}");
    }

    #[tokio::test]
    async fn grpc_methods_are_guarded_by_path_and_unknown_ones_refused() {
        let c = cluster("awzy-policy");
        let rules = vec![
            ("/svc/Read".to_string(), Rule::Allow(Allowed::only(&["awzy-system/awzy-pdp"]))),
            ("/svc/BreakGlass".to_string(), Rule::Exempt),
        ];
        let app = Router::new()
            .route("/svc/{method}", axum::routing::post(|| async { "ok" }))
            .route_layer(from_fn_with_state(PathGuard::new(c.verifier.clone(), rules), require_by_path));
        let call = |path: &'static str, auth: Option<String>| {
            let app = app.clone();
            async move {
                let mut req = HttpRequest::post(path);
                if let Some(a) = auth {
                    req = req.header(header::AUTHORIZATION, a);
                }
                app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap().status()
            }
        };
        assert_eq!(call("/svc/Read", None).await, StatusCode::UNAUTHORIZED);
        assert_eq!(call("/svc/Read", Some(token(&c, PDP, json!({ "aud": ["awzy-policy"] })))).await, StatusCode::OK);
        assert_eq!(call("/svc/BreakGlass", None).await, StatusCode::OK, "exempt: its own signatures authenticate it");
        let pdp = Some(token(&c, PDP, json!({ "aud": ["awzy-policy"] })));
        assert_eq!(call("/svc/Publish", pdp).await, StatusCode::FORBIDDEN, "a method no rule names is refused");
    }

    #[test]
    fn grpc_calls_carry_the_token() {
        use tonic::service::Interceptor;
        let dir = std::env::temp_dir().join(format!("awzy-service-auth-grpc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("awzy-pdp"), "t0k3n").unwrap();
        let mut interceptor =
            TokenInterceptor { audience: "awzy-pdp".into(), tokens: Arc::new(TokenSource::in_dir(&dir)) };
        let req = interceptor.call(tonic::Request::new(())).unwrap();
        assert_eq!(req.metadata().get("authorization").unwrap(), "Bearer t0k3n");
        let mut off = TokenInterceptor { audience: "awzy-pdp".into(), tokens: Arc::new(TokenSource::off()) };
        assert!(off.call(tonic::Request::new(())).unwrap().metadata().get("authorization").is_none());
        std::fs::remove_dir_all(dir).unwrap();
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

    #[tokio::test]
    async fn a_call_the_server_did_not_answer_is_tried_once_more() {
        for code in [tonic::Code::Unavailable, tonic::Code::Cancelled, tonic::Code::Internal, tonic::Code::Unknown] {
            let tries = std::cell::Cell::new(0);
            let res = retry_once_if_unanswered(|| {
                tries.set(tries.get() + 1);
                let first = tries.get() == 1;
                async move {
                    if first {
                        Err(tonic::Status::new(code, "connection closed"))
                    } else {
                        Ok("allow")
                    }
                }
            })
            .await;
            assert_eq!(res.unwrap(), "allow", "{code:?}");
            assert_eq!(tries.get(), 2);
        }
    }

    #[tokio::test]
    async fn only_once_and_never_after_the_server_answered() {
        let tries = std::cell::Cell::new(0);
        let res: Result<(), _> = retry_once_if_unanswered(|| {
            tries.set(tries.get() + 1);
            async { Err(tonic::Status::unavailable("still down")) }
        })
        .await;
        assert_eq!(res.unwrap_err().code(), tonic::Code::Unavailable);
        assert_eq!(tries.get(), 2, "one retry, not more");

        let tries = std::cell::Cell::new(0);
        let res: Result<(), _> = retry_once_if_unanswered(|| {
            tries.set(tries.get() + 1);
            async { Err(tonic::Status::invalid_argument("no action")) }
        })
        .await;
        assert_eq!(res.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(tries.get(), 1);
    }
}

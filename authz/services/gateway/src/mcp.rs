//! The MCP proxy: `/mcp/{upstream}` in front of an MCP server (streamable HTTP).
//!
//! Every request needs a valid token. A `tools/call` is checked with awzy-pdp as action `call`
//! on `McpTool::"<upstream>/<tool>"`, and, for a tool configured as acting on a resource, as
//! `read` or `write` on `Resource::"<id from its arguments>"` too (ADR 0048). In enforce mode a deny or challenge comes back as an MCP
//! tool error the model can read, and the upstream never sees the call. In monitor mode, and
//! for every other MCP message, the request is forwarded unchanged and the response streamed
//! back. The caller's token is never forwarded upstream.

use awzy_service_auth::AuthChannel;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use awzy_authz_proto::{
    authz::v1::{
        authorizer_client::AuthorizerClient, CheckRequest, CheckResponse, EnforcementMode, Entity,
        EntityRef as WireRef, Verdict,
    },
    entity_from_json, json_to_struct,
};
use awzy_model::{entity_type, EntityRef};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderValue, Method, Request, Response, StatusCode, Uri},
    routing::any,
    Router,
};
use bytes::Bytes;
use http_body_util::Full;
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use serde_json::{json, Value};

use awzy_identity::{Accessor, IdentityError, Verifier};

use crate::{GatewayConfig, UpstreamConfig};

const MAX_BODY: usize = 4 * 1024 * 1024;
const REQUEST_HEADERS: &[&str] = &["content-type", "accept", "mcp-session-id", "mcp-protocol-version", "last-event-id"];
const RESPONSE_HEADERS: &[&str] = &["content-type", "mcp-session-id", "cache-control"];

#[derive(Clone)]
pub struct GatewayState(Arc<Inner>);

struct Inner {
    config: GatewayConfig,
    verifier: Verifier,
    pdp: AuthorizerClient<AuthChannel>,
    upstream: Client<HttpConnector, Full<Bytes>>,
    /// The last mode the PDP reported. If the PDP becomes unreachable, a monitoring install
    /// keeps allowing; an enforcing one (or one never heard from) falls back to `outage`.
    last_seen_monitor: AtomicBool,
    /// When the PDP last allowed each (caller, tool), for replaying during an outage.
    allows: AllowCache,
}

/// Recent allows, keyed by everything that identified the call: subject, the person it acted
/// for, and the tool. Bounded; entries older than the TTL are dropped when it fills up.
struct AllowCache {
    ttl: std::time::Duration,
    entries: std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
}

const ALLOW_CACHE_MAX: usize = 100_000;

impl AllowCache {
    fn key(accessor: &Accessor, tool_id: &str) -> String {
        let person = accessor.on_behalf_of.as_ref().map(|p| format!("{}::{}", p.kind, p.id)).unwrap_or_default();
        format!("{}::{}|{person}|{tool_id}", accessor.subject.kind, accessor.subject.id)
    }

    fn record(&self, key: String) {
        let mut entries = self.entries.lock().expect("allow cache lock");
        if entries.len() >= ALLOW_CACHE_MAX {
            let ttl = self.ttl;
            entries.retain(|_, at| at.elapsed() < ttl);
            if entries.len() >= ALLOW_CACHE_MAX {
                return;
            }
        }
        entries.insert(key, std::time::Instant::now());
    }

    /// How long ago the PDP allowed this call, if within the TTL.
    fn recent(&self, key: &str) -> Option<std::time::Duration> {
        let entries = self.entries.lock().expect("allow cache lock");
        entries.get(key).map(|at| at.elapsed()).filter(|age| *age < self.ttl)
    }
}

impl GatewayState {
    pub fn new(config: GatewayConfig, pdp: AuthorizerClient<AuthChannel>) -> anyhow::Result<Self> {
        config.validate()?;
        let verifier = Verifier::new(&config.token)?;
        let upstream = Client::builder(TokioExecutor::new()).build_http();
        let allows = AllowCache {
            ttl: std::time::Duration::from_secs(config.outage.cached_allow_ttl_secs),
            entries: Default::default(),
        };
        Ok(Self(Arc::new(Inner { config, verifier, pdp, upstream, last_seen_monitor: AtomicBool::new(false), allows })))
    }
}

pub fn router(state: GatewayState) -> Router {
    Router::new().route("/mcp/{upstream}", any(proxy)).with_state(state)
}

async fn proxy(State(state): State<GatewayState>, Path(upstream): Path<String>, req: Request<Body>) -> Response<Body> {
    let Some(up) = state.0.config.upstream(&upstream).cloned() else {
        return plain(StatusCode::NOT_FOUND, format!("no upstream MCP server named {upstream:?}"));
    };
    let (parts, body) = req.into_parts();
    let authorization = parts.headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    let registry: &dyn awzy_identity::ServiceAccounts = match state.0.config.service_account_registry {
        crate::config::Registry::Config => &state.0.config.service_accounts,
        crate::config::Registry::AwzyContext => &awzy_identity::DeferToPdp,
    };
    let accessor = match state.0.verifier.accessor(authorization, registry).await {
        Ok(a) => a,
        Err(e @ IdentityError::UnregisteredServiceAccount(_)) => return plain(StatusCode::FORBIDDEN, e.to_string()),
        Err(e) => {
            let mut res = plain(StatusCode::UNAUTHORIZED, e.to_string());
            res.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
            return res;
        }
    };
    let Ok(body) = axum::body::to_bytes(body, MAX_BODY).await else {
        return plain(StatusCode::PAYLOAD_TOO_LARGE, "request body too large".into());
    };

    if parts.method == Method::POST {
        match serde_json::from_slice::<Value>(&body) {
            Ok(Value::Array(batch)) if batch.iter().any(is_tool_call) => {
                return jsonrpc_error(&Value::Null, -32600, "batched tools/call isn't supported by awzy-gateway");
            }
            Ok(msg) if is_tool_call(&msg) => {
                if let Some(blocked) = authorize(&state, &up, &accessor, &msg).await {
                    return blocked;
                }
            }
            _ => {}
        }
    }
    forward(&state, &up, parts, body).await
}

fn is_tool_call(msg: &Value) -> bool {
    msg.get("method").and_then(Value::as_str) == Some("tools/call")
}

/// Checks one `tools/call`. Returns the response to send instead of forwarding, if blocked.
async fn authorize(
    state: &GatewayState,
    up: &UpstreamConfig,
    accessor: &Accessor,
    msg: &Value,
) -> Option<Response<Body>> {
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let Some(tool) = msg.pointer("/params/name").and_then(Value::as_str) else {
        return Some(jsonrpc_error(&id, -32602, "tools/call needs params.name"));
    };
    let tool_id = format!("{}/{}", up.name, tool);
    let mut entities: Vec<Entity> = accessor.entities.iter().map(entity_from_json).collect();
    entities.push(entity_from_json(&json!({
        "uid": { "type": entity_type::MCP_TOOL, "id": tool_id },
        "attrs": { "sensitivity": up.sensitivity(tool) },
        "parents": [{ "type": entity_type::MCP_SERVER, "id": up.name }],
    })));
    let call = Check {
        action: "call",
        resource: WireRef { r#type: entity_type::MCP_TOOL.into(), id: tool_id.clone() },
        what: format!("the call to `{tool_id}`"),
        cache_key: AllowCache::key(accessor, &tool_id),
    };
    if let Some(blocked) = check(state, up, accessor, tool, &id, call, entities).await {
        return Some(blocked);
    }

    // A tool that acts on one resource is also checked as that action on the resource, which
    // the PDP loads from awzy-context by ID (ADR 0048).
    let rt = up.resource_tools.get(tool)?;
    let Some(resource) = msg.pointer(&format!("/params/arguments/{}", rt.argument)).and_then(Value::as_str) else {
        return Some(jsonrpc_error(&id, -32602, &format!("`{tool_id}` needs the argument {:?}", rt.argument)));
    };
    let on_resource = Check {
        action: &rt.action,
        resource: WireRef { r#type: entity_type::RESOURCE.into(), id: resource.into() },
        what: format!("`{}` on `{resource}` through `{tool_id}`", rt.action),
        cache_key: format!("{}|{}|{resource}", AllowCache::key(accessor, &tool_id), rt.action),
    };
    let entities = accessor.entities.iter().map(entity_from_json).collect();
    check(state, up, accessor, tool, &id, on_resource, entities).await
}

/// One PDP check for a tool call.
struct Check<'a> {
    action: &'a str,
    resource: WireRef,
    /// How messages name what was checked, e.g. "the call to `drive/read_file`".
    what: String,
    /// What a recent allow is remembered under, for replay during an outage.
    cache_key: String,
}

/// Asks the PDP and applies the mode. Returns the response to send instead of forwarding, if
/// blocked.
async fn check(
    state: &GatewayState,
    up: &UpstreamConfig,
    accessor: &Accessor,
    tool: &str,
    id: &Value,
    c: Check<'_>,
    entities: Vec<Entity>,
) -> Option<Response<Body>> {
    let req = CheckRequest {
        subject: Some(wire_ref(&accessor.subject)),
        action: c.action.into(),
        resource: Some(c.resource.clone()),
        context: Some(json_to_struct(json!({ "client": "mcp" }).as_object().unwrap().clone())),
        on_behalf_of: accessor.on_behalf_of.as_ref().map(wire_ref),
        entities,
        human_present: accessor.human_present,
        always_enforced: false,
    };
    match state.0.pdp.clone().check(req).await {
        Ok(res) => {
            let res = res.into_inner();
            // UNSPECIFIED counts as enforce: never skip enforcement by accident.
            let monitor = res.mode() == EnforcementMode::Monitor;
            state.0.last_seen_monitor.store(monitor, Ordering::Release);
            match (res.verdict(), monitor) {
                (Verdict::Allow, _) => {
                    state.0.allows.record(c.cache_key);
                    None
                }
                (_, true) => {
                    tracing::info!(decision = %res.decision_qid, what = %c.what, verdict = ?res.verdict(), "monitor mode: forwarding a call that would be blocked");
                    None
                }
                _ => Some(blocked(id, &c.what, &res)),
            }
        }
        Err(status) if state.0.last_seen_monitor.load(Ordering::Acquire) => {
            tracing::warn!(%status, what = %c.what, "awzy-pdp unreachable; install was monitoring, forwarding");
            None
        }
        Err(status) => outage(state, up, tool, &c, id, &status),
    }
}

/// The PDP is unreachable and the install is enforcing: replay a recent allow for a listed
/// fail-open tool, deny everything else (ADR 0030). A resource check replays only an allow for
/// the same resource.
fn outage(
    state: &GatewayState,
    up: &UpstreamConfig,
    tool: &str,
    c: &Check<'_>,
    id: &Value,
    status: &tonic::Status,
) -> Option<Response<Body>> {
    let outage = &state.0.config.outage;
    let tier = up.listed_sensitivity(tool);
    let fail_open = tier.is_some_and(|t| {
        outage.fail_open_tiers.iter().any(|f| f == t) && !crate::config::ALWAYS_FAIL_CLOSED.contains(&t)
    });
    let replay = if fail_open { state.0.allows.recent(&c.cache_key) } else { None };
    match replay {
        Some(age) => {
            tracing::warn!(%status, what = %c.what, tier = ?tier, age_secs = age.as_secs(), "awzy-pdp unreachable; replaying a recent allow");
            None
        }
        None => {
            tracing::warn!(%status, what = %c.what, tier = ?tier, "awzy-pdp unreachable; denying");
            let why = match tier {
                None => "it has no sensitivity of its own in the gateway's config, so it fails closed".to_string(),
                Some(t) if !fail_open => format!("its tier ({t}) fails closed"),
                Some(_) => "no recent allow for this caller could be replayed".to_string(),
            };
            Some(tool_error(
                id,
                format!(
                    "awzy couldn't reach its policy decision point, so {} was not made: {why}. Try again shortly.",
                    c.what
                ),
                json!({ "awzy/verdict": "unavailable" }),
            ))
        }
    }
}

/// An MCP tool error: the model sees why the call didn't run.
fn blocked(id: &Value, what: &str, res: &CheckResponse) -> Response<Body> {
    let policies = if res.policies.is_empty() { "no policy permits it".to_string() } else { res.policies.join(", ") };
    let (verdict, text) = match res.verdict() {
        Verdict::Challenge => {
            let kinds: Vec<String> = res
                .challenges()
                .map(|c| c.as_str_name().trim_start_matches("CHALLENGE_KIND_").to_lowercase())
                .collect();
            let kinds = kinds.join(" and ");
            let next = if res.challenge_qid.is_empty() {
                "No challenge could be opened, so the call was not made. Try again shortly.".to_string()
            } else {
                format!(
                    "The call was not made. Challenge {} is waiting for an answer; once it's passed, \
                     retry the same call.",
                    res.challenge_qid
                )
            };
            (
                "challenge",
                format!(
                    "awzy requires {kinds} for {what} (decision {}, policies: {policies}). {next}",
                    res.decision_qid
                ),
            )
        }
        _ => (
            "deny",
            format!(
                "awzy denied {what} (decision {}; {policies}). If this access is needed, ask the person \
                 you're working for or a security admin.",
                res.decision_qid
            ),
        ),
    };
    let mut meta = json!({ "awzy/decision_qid": res.decision_qid, "awzy/verdict": verdict });
    if !res.challenge_qid.is_empty() {
        meta["awzy/challenge_qid"] = json!(res.challenge_qid);
    }
    tool_error(id, text, meta)
}

fn tool_error(id: &Value, text: String, meta: Value) -> Response<Body> {
    json_response(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "isError": true, "content": [{ "type": "text", "text": text }], "_meta": meta },
    }))
}

fn jsonrpc_error(id: &Value, code: i64, message: &str) -> Response<Body> {
    json_response(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }))
}

fn json_response(v: Value) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(v.to_string()))
        .expect("static response")
}

fn plain(status: StatusCode, text: String) -> Response<Body> {
    Response::builder().status(status).body(Body::from(text)).expect("static response")
}

async fn forward(
    state: &GatewayState,
    up: &UpstreamConfig,
    parts: axum::http::request::Parts,
    body: Bytes,
) -> Response<Body> {
    let uri = match upstream_uri(&up.url, parts.uri.query()) {
        Ok(u) => u,
        Err(e) => return plain(StatusCode::BAD_GATEWAY, format!("bad upstream URL for {:?}: {e}", up.name)),
    };
    let mut builder = Request::builder().method(parts.method).uri(uri);
    for name in REQUEST_HEADERS {
        if let Some(v) = parts.headers.get(*name) {
            builder = builder.header(*name, v);
        }
    }
    let Ok(req) = builder.body(Full::new(body)) else {
        return plain(StatusCode::BAD_REQUEST, "could not build the upstream request".into());
    };
    match state.0.upstream.request(req).await {
        Ok(res) => {
            let (p, b) = res.into_parts();
            let mut out = Response::builder().status(p.status);
            for name in RESPONSE_HEADERS {
                if let Some(v) = p.headers.get(*name) {
                    out = out.header(*name, v);
                }
            }
            out.body(Body::new(b)).unwrap_or_else(|_| plain(StatusCode::BAD_GATEWAY, "bad upstream response".into()))
        }
        Err(e) => plain(StatusCode::BAD_GATEWAY, format!("upstream MCP server {:?} unreachable: {e}", up.name)),
    }
}

fn upstream_uri(url: &str, query: Option<&str>) -> Result<Uri, axum::http::uri::InvalidUri> {
    match query {
        Some(q) => format!("{url}?{q}").parse(),
        None => url.parse(),
    }
}

fn wire_ref(r: &EntityRef) -> WireRef {
    WireRef { r#type: r.kind.clone(), id: r.id.clone() }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::routing::post;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use tower::ServiceExt;

    use super::*;
    use crate::{ServiceAccountConfig, TokenConfig, TokenKey};

    const SECRET_ENV: &str = "AWZY_TEST_GATEWAY_SECRET";
    const SECRET: &str = "test-secret";

    /// The fake upstream records each tool it ran and whether it saw an Authorization header.
    #[derive(Clone, Default)]
    struct Upstream(Arc<Mutex<Vec<String>>>, Arc<AtomicBool>);

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    async fn start_upstream(seen: Upstream) -> String {
        let app = Router::new().route(
            "/mcp",
            post(move |headers: axum::http::HeaderMap, body: String| {
                let seen = seen.clone();
                async move {
                    if headers.contains_key(header::AUTHORIZATION) {
                        seen.1.store(true, Ordering::Release);
                    }
                    let msg: Value = serde_json::from_str(&body).unwrap();
                    let result = match msg["params"]["name"].as_str() {
                        Some(tool) => {
                            seen.0.lock().unwrap().push(tool.to_string());
                            json!({ "content": [{ "type": "text", "text": format!("ran {tool}") }] })
                        }
                        None => json!({ "tools": [] }),
                    };
                    axum::Json(json!({ "jsonrpc": "2.0", "id": msg["id"], "result": result }))
                }
            }),
        );
        format!("{}/mcp", serve(app).await)
    }

    /// A fake awzy-context that knows two documents: internal notes and the crown-jewel plan.
    async fn start_context() -> String {
        let app = Router::new().route(
            "/v1/resources/{id}/entities",
            axum::routing::get(|Path(id): Path<String>| async move {
                let tier = match id.as_str() {
                    "synthetic:doc:notes" => "internal",
                    "synthetic:doc:plan" => "crown_jewels",
                    _ => return Err(StatusCode::NOT_FOUND),
                };
                Ok(axum::Json(json!({ "assignees": [], "entities": [{ "uid": { "type": "Resource", "id": id },
                    "attrs": { "sensitivity": tier, "unclassified": false }, "parents": [] }] })))
            }),
        );
        serve(app).await
    }

    async fn gateway(mode: awzy_model::EnforcementMode, upstream_url: String) -> Router {
        gateway_with_outage_switch(mode, upstream_url).await.0
    }

    /// The gateway, and a switch that makes the PDP answer 503 as if it were down.
    async fn gateway_with_outage_switch(
        mode: awzy_model::EnforcementMode,
        upstream_url: String,
    ) -> (Router, Arc<AtomicBool>) {
        std::env::set_var(SECRET_ENV, SECRET);
        let enforcement = Arc::new(awzy_pdp::Enforcement::fixed(mode));
        let down = Arc::new(AtomicBool::new(false));
        let switch = down.clone();
        let pdp_app = awzy_pdp::router(
            awzy_pdp::AppState::new(awzy_pdp::shared(awzy_pdp::Pdp::with_defaults().unwrap()))
                .with_enforcement(enforcement)
                .with_resources(Arc::new(awzy_pdp::resources::Resources::from_context(start_context().await))),
        )
        .layer(axum::middleware::from_fn(move |req: Request<Body>, next: axum::middleware::Next| {
            let down = switch.clone();
            async move {
                if down.load(Ordering::Acquire) {
                    return axum::response::IntoResponse::into_response(StatusCode::SERVICE_UNAVAILABLE);
                }
                next.run(req).await
            }
        }));
        let channel = tonic::transport::Channel::from_shared(serve(pdp_app).await).unwrap().connect().await.unwrap();
        let pdp = AuthorizerClient::new(awzy_service_auth::grpc_channel(channel, "awzy-pdp"));
        let config = GatewayConfig {
            token: TokenConfig {
                issuer: "https://idp.test".into(),
                audience: "awzy-gateway".into(),
                jwks_ca_file: None,
                subject_claim: None,
                key: TokenKey::Hs256SecretEnv(SECRET_ENV.into()),
            },
            upstreams: vec![UpstreamConfig {
                name: "drive".into(),
                url: upstream_url,
                default_sensitivity: "internal".into(),
                tools: [
                    ("delete_file".to_string(), "confidential".to_string()),
                    ("read_file".to_string(), "internal".to_string()),
                    ("read_document".to_string(), "internal".to_string()),
                ]
                .into(),
                resource_tools: [(
                    "read_document".to_string(),
                    crate::config::ResourceTool { argument: "id".into(), action: "read".into() },
                )]
                .into(),
            }],
            service_accounts: vec![ServiceAccountConfig {
                id: "sales-genie".into(),
                owner: "bob".into(),
                ai: true,
                groups: vec!["engineering".into()],
            }],
            outage: crate::config::OutageConfig {
                fail_open_tiers: vec!["internal".into()],
                cached_allow_ttl_secs: 300,
            },
            service_account_registry: crate::config::Registry::Config,
        };
        (router(GatewayState::new(config, pdp).unwrap()), down)
    }

    fn token(sub: &str, act: Option<&str>, groups: &[&str]) -> String {
        let mut claims = json!({
            "iss": "https://idp.test", "aud": "awzy-gateway", "sub": sub, "groups": groups,
            "exp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 300,
        });
        if let Some(a) = act {
            claims["act"] = json!({ "sub": a });
        }
        encode(&Header::default(), &claims, &EncodingKey::from_secret(SECRET.as_bytes())).unwrap()
    }

    async fn call(app: &Router, token: Option<&str>, tool: &str) -> (StatusCode, Value) {
        call_with(app, token, tool, json!({})).await
    }

    async fn call_with(app: &Router, token: Option<&str>, tool: &str, arguments: Value) -> (StatusCode, Value) {
        let body = json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": tool, "arguments": arguments } });
        let mut req = Request::post("/mcp/drive")
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream");
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let res = app.clone().oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    #[tokio::test]
    async fn enforce_blocks_denied_calls_before_the_upstream() {
        let seen = Upstream::default();
        let app = gateway(awzy_model::EnforcementMode::Enforce, start_upstream(seen.clone()).await).await;

        // bob (engineering) may call internal tools but not the confidential delete_file.
        let (_, ok) = call(&app, Some(&token("bob", None, &["engineering"])), "read_file").await;
        assert_eq!(ok["result"]["content"][0]["text"], "ran read_file");
        let (_, denied) = call(&app, Some(&token("bob", None, &["engineering"])), "delete_file").await;
        assert_eq!(denied["result"]["isError"], true);
        assert_eq!(denied["result"]["_meta"]["awzy/verdict"], "deny");
        assert!(denied["result"]["_meta"]["awzy/decision_qid"].as_str().unwrap().starts_with("decision_"));
        assert_eq!(denied["id"], 7);

        // alice (security) may call any tool.
        let (_, ok) = call(&app, Some(&token("alice", None, &["security"])), "delete_file").await;
        assert_eq!(ok["result"]["content"][0]["text"], "ran delete_file");

        assert_eq!(
            *seen.0.lock().unwrap(),
            vec!["read_file", "delete_file"],
            "bob's delete never reached the upstream"
        );
        assert!(!seen.1.load(Ordering::Acquire), "the caller's token is never forwarded upstream");
    }

    #[tokio::test]
    async fn delegated_agent_gets_at_most_what_the_person_can() {
        let seen = Upstream::default();
        let app = gateway(awzy_model::EnforcementMode::Enforce, start_upstream(seen.clone()).await).await;
        // sales-genie (engineering) acting for alice (security): alice could delete, the agent can't.
        let (_, denied) = call(&app, Some(&token("alice", Some("sales-genie"), &["security"])), "delete_file").await;
        assert_eq!(denied["result"]["isError"], true);
        let (_, ok) = call(&app, Some(&token("alice", Some("sales-genie"), &["security"])), "read_file").await;
        assert_eq!(ok["result"]["content"][0]["text"], "ran read_file");
    }

    #[tokio::test]
    async fn monitor_forwards_what_enforce_would_block() {
        let seen = Upstream::default();
        let app = gateway(awzy_model::EnforcementMode::Monitor, start_upstream(seen.clone()).await).await;
        let (_, res) = call(&app, Some(&token("bob", None, &["engineering"])), "delete_file").await;
        assert_eq!(res["result"]["content"][0]["text"], "ran delete_file");
    }

    #[tokio::test]
    async fn rejects_missing_invalid_and_unregistered_identities() {
        let app = gateway(awzy_model::EnforcementMode::Enforce, start_upstream(Upstream::default()).await).await;
        assert_eq!(call(&app, None, "read_file").await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(call(&app, Some("not-a-jwt"), "read_file").await.0, StatusCode::UNAUTHORIZED);
        let forged = encode(
            &Header::default(),
            &json!({ "iss": "https://idp.test", "aud": "awzy-gateway", "sub": "bob", "exp": 4_000_000_000_u64 }),
            &EncodingKey::from_secret(b"wrong-secret"),
        )
        .unwrap();
        assert_eq!(call(&app, Some(&forged), "read_file").await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(call(&app, Some(&token("bob", Some("rogue-bot"), &[])), "read_file").await.0, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn outage_replays_only_recent_allows_for_listed_fail_open_tools() {
        let seen = Upstream::default();
        let (app, pdp_down) =
            gateway_with_outage_switch(awzy_model::EnforcementMode::Enforce, start_upstream(seen.clone()).await).await;
        let alice = token("alice", None, &["security"]);
        let bob = token("bob", None, &["engineering"]);

        // While the PDP is up, alice reads and deletes, and calls an unlisted tool.
        for tool in ["read_file", "delete_file", "list_files"] {
            let (_, ok) = call(&app, Some(&alice), tool).await;
            assert_eq!(ok["result"]["content"][0]["text"], format!("ran {tool}"));
        }

        pdp_down.store(true, Ordering::Release);
        let (_, replayed) = call(&app, Some(&alice), "read_file").await;
        assert_eq!(
            replayed["result"]["content"][0]["text"], "ran read_file",
            "internal fails open with a recent allow"
        );

        let (_, confidential) = call(&app, Some(&alice), "delete_file").await;
        assert_eq!(confidential["result"]["_meta"]["awzy/verdict"], "unavailable");
        assert!(confidential["result"]["content"][0]["text"].as_str().unwrap().contains("(confidential) fails closed"));

        let (_, unlisted) = call(&app, Some(&alice), "list_files").await;
        assert!(
            unlisted["result"]["content"][0]["text"].as_str().unwrap().contains("no sensitivity of its own"),
            "{unlisted}"
        );

        let (_, never_allowed) = call(&app, Some(&bob), "read_file").await;
        assert!(
            never_allowed["result"]["content"][0]["text"].as_str().unwrap().contains("no recent allow"),
            "bob was never allowed"
        );

        assert_eq!(
            *seen.0.lock().unwrap(),
            vec!["read_file", "delete_file", "list_files", "read_file"],
            "during the outage only alice's replayed read reached the upstream"
        );
    }

    #[tokio::test]
    async fn a_resource_tool_is_checked_on_the_resource_it_names() {
        let seen = Upstream::default();
        let (app, pdp_down) =
            gateway_with_outage_switch(awzy_model::EnforcementMode::Enforce, start_upstream(seen.clone()).await).await;
        let bob = token("bob", None, &["engineering"]);
        let read = |id: &str| json!({ "id": id });

        // The tool is internal, so bob may call it; the document decides.
        let (_, ok) = call_with(&app, Some(&bob), "read_document", read("synthetic:doc:notes")).await;
        assert_eq!(ok["result"]["content"][0]["text"], "ran read_document");
        let (_, denied) = call_with(&app, Some(&bob), "read_document", read("synthetic:doc:plan")).await;
        assert_eq!(denied["result"]["_meta"]["awzy/verdict"], "deny");
        let text = denied["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("`read` on `synthetic:doc:plan` through `drive/read_document`"), "{text}");

        let (_, missing) = call_with(&app, Some(&bob), "read_document", json!({})).await;
        assert_eq!(missing["error"]["code"], -32602, "the resource argument is required");

        // During an outage, only a recent allow for the same document is replayed.
        pdp_down.store(true, Ordering::Release);
        let (_, replayed) = call_with(&app, Some(&bob), "read_document", read("synthetic:doc:notes")).await;
        assert_eq!(replayed["result"]["content"][0]["text"], "ran read_document");
        let (_, other) = call_with(&app, Some(&bob), "read_document", read("synthetic:doc:other")).await;
        assert_eq!(other["result"]["_meta"]["awzy/verdict"], "unavailable");

        assert_eq!(*seen.0.lock().unwrap(), vec!["read_document", "read_document"], "only the notes, twice");
    }

    #[tokio::test]
    async fn monitor_forwards_a_resource_call_enforce_would_block() {
        let seen = Upstream::default();
        let app = gateway(awzy_model::EnforcementMode::Monitor, start_upstream(seen.clone()).await).await;
        let bob = token("bob", None, &["engineering"]);
        let (_, res) = call_with(&app, Some(&bob), "read_document", json!({ "id": "synthetic:doc:plan" })).await;
        assert_eq!(res["result"]["content"][0]["text"], "ran read_document");
    }

    #[test]
    fn resource_tools_need_an_argument_and_read_or_write() {
        let config: GatewayConfig = serde_json::from_value(json!({
            "token": { "issuer": "i", "audience": "a", "hs256_secret_env": "X" },
            "upstreams": [{ "name": "drive", "url": "http://x/mcp",
                            "resource_tools": { "read_document": { "argument": "id", "action": "delete" } } }],
        }))
        .unwrap();
        assert!(config.validate().unwrap_err().to_string().contains("read or write"));
    }

    #[test]
    fn high_sensitivity_tiers_can_never_fail_open() {
        let config: GatewayConfig = serde_json::from_value(json!({
            "token": { "issuer": "i", "audience": "a", "hs256_secret_env": "X" },
            "upstreams": [],
            "outage": { "fail_open_tiers": ["internal", "crown_jewels"] },
        }))
        .unwrap();
        assert!(config.validate().unwrap_err().to_string().contains("crown_jewels"));
    }
}

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
use serde_json::{json, Value};

use awzy_identity::{IdentityError, Principal, Verifier};

use crate::{
    containments::{Containments, Replay},
    fallback::Fallback,
    GatewayConfig, UpstreamConfig,
};

const MAX_BODY: usize = 4 * 1024 * 1024;
const REQUEST_HEADERS: &[&str] = &["content-type", "accept", "mcp-session-id", "mcp-protocol-version", "last-event-id"];
const RESPONSE_HEADERS: &[&str] = &["content-type", "mcp-session-id", "cache-control"];

#[derive(Clone)]
pub struct GatewayState(Arc<Inner>);

struct Inner {
    config: GatewayConfig,
    verifier: Verifier,
    pdp: AuthorizerClient<AuthChannel>,
    upstream: awzy_http::Client,
    /// The last mode the PDP reported. If the PDP becomes unreachable, a monitoring install
    /// keeps allowing; an enforcing one (or one never heard from) falls back to `outage`.
    last_seen_monitor: AtomicBool,
    /// When the PDP last allowed each (caller, tool), for replaying during an outage.
    allows: AllowCache,
    /// Who is contained, so an outage never replays an allow for them (ADR 0030).
    containments: Arc<Containments>,
    /// Each tool's tier, and which tiers fail open during an outage (ADR 0089).
    fallback: Arc<Fallback>,
    /// The MCP client each session's `initialize` named, for `context.mcp_client` (ADR 0081).
    sessions: SessionClients,
    /// Where forwarded tool calls are recorded as activity (ADR 0084).
    activity: awzy_activity::ActivityLog,
}

/// The client name each MCP session started with, by session ID, for the principal that started
/// it. It's what the client says it is: a hint for policy, not proof. Bounded like the allows.
#[derive(Default)]
struct SessionClients(std::sync::Mutex<std::collections::HashMap<String, (String, String, std::time::Instant)>>);

const SESSION_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 3600);
const SESSIONS_MAX: usize = 100_000;
/// The longest client name passed on.
const CLIENT_NAME_MAX: usize = 64;

impl SessionClients {
    fn record(&self, session: &str, principal: &Principal, client: &str) {
        let mut sessions = self.0.lock().expect("sessions lock");
        if sessions.len() >= SESSIONS_MAX {
            sessions.retain(|_, (.., at)| at.elapsed() < SESSION_TTL);
            if sessions.len() >= SESSIONS_MAX {
                return;
            }
        }
        sessions.insert(session.to_string(), (principal_key(principal), client.to_string(), std::time::Instant::now()));
    }

    /// The session's client, if it's this principal's session and not too old.
    fn client(&self, session: &str, principal: &Principal) -> Option<String> {
        let sessions = self.0.lock().expect("sessions lock");
        let (owner, client, at) = sessions.get(session)?;
        (*owner == principal_key(principal) && at.elapsed() < SESSION_TTL).then(|| client.clone())
    }
}

/// `Type::id|Type::id`: the principal and the person it acts for.
fn principal_key(principal: &Principal) -> String {
    let human = principal.on_behalf_of.as_ref().map(|p| format!("{}::{}", p.kind, p.id)).unwrap_or_default();
    format!("{}::{}|{human}", principal.subject.kind, principal.subject.id)
}

/// The client name in an `initialize` request, cut to printable ASCII and [`CLIENT_NAME_MAX`].
fn initialize_client(msg: &Value) -> Option<String> {
    if msg.get("method").and_then(Value::as_str) != Some("initialize") {
        return None;
    }
    let name: String = msg
        .pointer("/params/clientInfo/name")?
        .as_str()?
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(CLIENT_NAME_MAX)
        .collect();
    Some(name.trim().to_string()).filter(|n| !n.is_empty())
}

/// Recent allows, keyed by everything that identified the call: principal, the person it acted
/// for, and the tool. Bounded; entries older than the TTL are dropped when it fills up.
struct AllowCache {
    ttl: std::time::Duration,
    entries: std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
}

const ALLOW_CACHE_MAX: usize = 100_000;

impl AllowCache {
    fn key(principal: &Principal, tool_id: &str) -> String {
        let human = principal.on_behalf_of.as_ref().map(|p| format!("{}::{}", p.kind, p.id)).unwrap_or_default();
        format!("{}::{}|{human}|{tool_id}", principal.subject.kind, principal.subject.id)
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

    /// Drops every allow for a call by, or for, one of `contained` (`Type::id`).
    fn forget(&self, contained: &std::collections::HashSet<String>) {
        if contained.is_empty() {
            return;
        }
        let mut entries = self.entries.lock().expect("allow cache lock");
        entries.retain(|key, _| {
            let mut parts = key.splitn(3, '|');
            let (principal, human) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
            !contained.contains(principal) && !contained.contains(human)
        });
    }
}

impl GatewayState {
    /// Without awzy-challenge: outages replay without regard to containments.
    pub fn new(config: GatewayConfig, pdp: AuthorizerClient<AuthChannel>) -> anyhow::Result<Self> {
        Self::with_containments(config, pdp, Arc::new(Containments::none()))
    }

    /// Following awzy-challenge's containments; call [`Self::forget_allows`] with each list.
    pub fn with_containments(
        config: GatewayConfig,
        pdp: AuthorizerClient<AuthChannel>,
        containments: Arc<Containments>,
    ) -> anyhow::Result<Self> {
        config.validate()?;
        let verifier = Verifier::new(&config.token)?;
        let upstream = awzy_http::client();
        let allows = AllowCache {
            ttl: std::time::Duration::from_secs(config.outage.cached_allow_ttl_secs),
            entries: Default::default(),
        };
        Ok(Self(Arc::new(Inner {
            config,
            verifier,
            pdp,
            upstream,
            last_seen_monitor: AtomicBool::new(false),
            allows,
            containments,
            fallback: Arc::new(Fallback::awaiting()),
            sessions: SessionClients::default(),
            activity: awzy_activity::ActivityLog::off(),
        })))
    }

    /// Records each forwarded tool call in awzy-intake (ADR 0084). Call before sharing the state.
    pub fn with_activity(mut self, activity: awzy_activity::ActivityLog) -> Self {
        Arc::get_mut(&mut self.0).expect("not shared yet").activity = activity;
        self
    }

    /// Takes tool tiers and fail-open tiers from `fallback`, which [`crate::fallback::follow`]
    /// keeps current. Without it, every tool is unclassified. Call before sharing the state.
    pub fn with_fallback(mut self, fallback: Arc<Fallback>) -> Self {
        Arc::get_mut(&mut self.0).expect("not shared yet").fallback = fallback;
        self
    }

    /// Drops cached allows for calls by, or for, the contained principals (`Type::id`).
    pub fn forget_allows(&self, contained: &std::collections::HashSet<String>) {
        self.0.allows.forget(contained);
    }
}

pub fn router(state: GatewayState) -> Router {
    Router::new()
        .route("/mcp/{upstream}", any(proxy))
        .route("/.well-known/oauth-protected-resource/mcp/{upstream}", axum::routing::get(resource_metadata))
        .with_state(state)
}

/// OAuth protected resource metadata (RFC 9728) for one upstream, so an MCP client can find the
/// IdP to sign in with on its own (ADR 0081). Only when the gateway knows its public URL.
async fn resource_metadata(State(state): State<GatewayState>, Path(upstream): Path<String>) -> Response<Body> {
    let (Some(_), Some(resource)) = (state.0.config.upstream(&upstream), resource_url(&state, &upstream)) else {
        return plain(StatusCode::NOT_FOUND, format!("no sign-in metadata for {upstream:?}"));
    };
    json_response(json!({
        "resource": resource,
        "authorization_servers": [state.0.config.token.issuer],
        "bearer_methods_supported": ["header"],
    }))
}

/// `https://<gateway>/mcp/<upstream>`, when the gateway knows its public URL.
fn resource_url(state: &GatewayState, upstream: &str) -> Option<String> {
    state.0.config.public_url.as_ref().map(|u| format!("{}/mcp/{upstream}", u.trim_end_matches('/')))
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
    let principal = match state.0.verifier.principal(authorization, registry).await {
        Ok(a) => a,
        Err(e @ IdentityError::UnregisteredServiceAccount(_)) => return plain(StatusCode::FORBIDDEN, e.to_string()),
        Err(e) => {
            let mut res = plain(StatusCode::UNAUTHORIZED, e.to_string());
            let challenge = match resource_url(&state, &upstream) {
                Some(resource) => {
                    let metadata = resource.replacen("/mcp/", "/.well-known/oauth-protected-resource/mcp/", 1);
                    format!(r#"Bearer error="invalid_token", resource_metadata="{metadata}""#)
                }
                None => "Bearer".to_string(),
            };
            if let Ok(v) = HeaderValue::from_str(&challenge) {
                res.headers_mut().insert(header::WWW_AUTHENTICATE, v);
            }
            return res;
        }
    };
    // Streamable HTTP uses only these. Anything else could carry a message the checks below never
    // see, to an upstream lenient enough to act on it.
    if ![Method::GET, Method::POST, Method::DELETE].contains(&parts.method) {
        return plain(StatusCode::METHOD_NOT_ALLOWED, format!("awzy-gateway doesn't forward {}", parts.method));
    }
    let Ok(body) = axum::body::to_bytes(body, MAX_BODY).await else {
        return plain(StatusCode::PAYLOAD_TOO_LARGE, "request body too large".into());
    };

    let session = parts.headers.get("mcp-session-id").and_then(|v| v.to_str().ok()).map(str::to_string);
    let mut initializing = None;
    let mut calling = None;
    if parts.method == Method::POST {
        match serde_json::from_slice::<Value>(&body) {
            Ok(Value::Array(batch)) if batch.iter().any(is_tool_call) => {
                return jsonrpc_error(&Value::Null, -32600, "batched tools/call isn't supported by awzy-gateway");
            }
            Ok(msg) if is_tool_call(&msg) => {
                let client = session.as_deref().and_then(|s| state.0.sessions.client(s, &principal));
                if let Some(blocked) = authorize(&state, &up, &principal, &msg, client.as_deref()).await {
                    return blocked;
                }
                let tool = msg.pointer("/params/name").and_then(Value::as_str).unwrap_or_default();
                calling = Some((format!("{}/{tool}", up.name), client));
            }
            Ok(msg @ Value::Object(_)) => initializing = initialize_client(&msg),
            Ok(Value::Array(_)) => {}
            // Fails closed: a body this parser refuses (nesting past its limit, a lone surrogate)
            // may still parse upstream, as a tools/call that was never checked.
            _ => return unreadable(),
        }
    }
    let res = forward(&state, &up, parts, body).await;
    // A forwarded tool call is activity, whatever the upstream made of it (ADR 0084). Blocked ones
    // are in the decision log already.
    if let Some((tool_id, client)) = calling {
        let tool = EntityRef::new(entity_type::MCP_TOOL, tool_id);
        let mut activity =
            awzy_activity::Activity::now(principal.subject.clone(), principal.on_behalf_of.clone(), "call", Some(tool))
                .with_outcome((!res.status().is_success()).then_some("failure"))
                .with_attribute("client", "mcp");
        if let Some(name) = client {
            activity = activity.with_attribute("mcp_client", name);
        }
        state.0.activity.record(activity);
    }
    // The upstream names the session in its answer to `initialize`.
    if let Some(client) = initializing {
        if let Some(id) = res.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            state.0.sessions.record(id, &principal, &client);
        }
    }
    res
}

fn is_tool_call(msg: &Value) -> bool {
    msg.get("method").and_then(Value::as_str) == Some("tools/call")
}

/// Checks one `tools/call`. Returns the response to send instead of forwarding, if blocked.
async fn authorize(
    state: &GatewayState,
    up: &UpstreamConfig,
    principal: &Principal,
    msg: &Value,
    client: Option<&str>,
) -> Option<Response<Body>> {
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let Some(tool) = msg.pointer("/params/name").and_then(Value::as_str) else {
        return Some(jsonrpc_error(&id, -32602, "tools/call needs params.name"));
    };
    let tool_id = format!("{}/{}", up.name, tool);
    let mut entities: Vec<Entity> = principal.entities.iter().map(entity_from_json).collect();
    entities.push(entity_from_json(&json!({
        "uid": { "type": entity_type::MCP_TOOL, "id": tool_id },
        "attrs": { "sensitivity": state.0.fallback.tier(&up.name, tool) },
        "parents": [{ "type": entity_type::MCP_SERVER, "id": up.name }],
    })));
    let call = ToolCheck {
        action: "call",
        resource: WireRef { r#type: entity_type::MCP_TOOL.into(), id: tool_id.clone() },
        what: format!("the call to `{tool_id}`"),
        cache_key: AllowCache::key(principal, &tool_id),
        client,
    };
    if let Some(blocked) = check(state, up, principal, tool, &id, call, entities).await {
        return Some(blocked);
    }

    // A tool that acts on one resource is also checked as that action on the resource, which
    // the PDP loads from awzy-context by ID (ADR 0048).
    let rt = up.resource_tools.get(tool)?;
    let Some(resource) = msg.pointer(&format!("/params/arguments/{}", rt.argument)).and_then(Value::as_str) else {
        return Some(jsonrpc_error(&id, -32602, &format!("`{tool_id}` needs the argument {:?}", rt.argument)));
    };
    let on_resource = ToolCheck {
        action: &rt.action,
        resource: WireRef { r#type: entity_type::RESOURCE.into(), id: resource.into() },
        what: format!("`{}` on `{resource}` through `{tool_id}`", rt.action),
        cache_key: format!("{}|{}|{resource}", AllowCache::key(principal, &tool_id), rt.action),
        client,
    };
    let entities = principal.entities.iter().map(entity_from_json).collect();
    check(state, up, principal, tool, &id, on_resource, entities).await
}

/// One PDP check for a tool call.
struct ToolCheck<'a> {
    action: &'a str,
    resource: WireRef,
    /// How messages name what was checked, e.g. "the call to `drive/read_file`".
    what: String,
    /// What a recent allow is remembered under, for replay during an outage.
    cache_key: String,
    /// The MCP client's name from its session's `initialize`, if the gateway saw it.
    client: Option<&'a str>,
}

/// `client` is `mcp` for every call through the gateway; `mcp_client` names the MCP client, as
/// its session's `initialize` did (ADR 0081).
fn check_context(client: Option<&str>) -> serde_json::Map<String, Value> {
    let mut context = serde_json::Map::new();
    context.insert("client".into(), json!("mcp"));
    if let Some(name) = client {
        context.insert("mcp_client".into(), json!(name));
    }
    context
}

/// Asks the PDP and applies the mode. Returns the response to send instead of forwarding, if
/// blocked.
async fn check(
    state: &GatewayState,
    up: &UpstreamConfig,
    principal: &Principal,
    tool: &str,
    id: &Value,
    c: ToolCheck<'_>,
    entities: Vec<Entity>,
) -> Option<Response<Body>> {
    let req = CheckRequest {
        principal: Some(wire_ref(&principal.subject)),
        action: c.action.into(),
        resource: Some(c.resource.clone()),
        context: Some(json_to_struct(check_context(c.client))),
        on_behalf_of: principal.on_behalf_of.as_ref().map(wire_ref),
        entities,
        human_present: principal.human_present,
        always_enforced: false,
    };
    // A PDP pod shutting down can break the connection under one check; ask again before
    // treating it as an outage.
    let ask = || {
        let (mut pdp, req) = (state.0.pdp.clone(), req.clone());
        async move { pdp.check(req).await }
    };
    match awzy_service_auth::retry_once_if_unanswered(ask).await {
        Ok(res) => {
            let res = res.into_inner();
            // UNSPECIFIED counts as enforce: never skip enforcement by accident.
            let monitor = res.mode() == EnforcementMode::Monitor;
            state.0.last_seen_monitor.store(monitor, Ordering::Release);
            match (res.verdict(), monitor) {
                (Verdict::Allow, _) => {
                    crate::metrics::FORWARDED.inc();
                    state.0.allows.record(c.cache_key);
                    None
                }
                (_, true) => {
                    crate::metrics::MONITOR_FORWARDED.inc();
                    tracing::info!(decision = %res.decision_qid, what = %c.what, verdict = ?res.verdict(), "monitor mode: forwarding a call that would be blocked");
                    None
                }
                _ => {
                    crate::metrics::BLOCKED.inc();
                    Some(blocked(id, &c.what, &res, state.0.config.portal_url.as_deref()))
                }
            }
        }
        Err(status) if state.0.last_seen_monitor.load(Ordering::Acquire) => {
            crate::metrics::OUTAGE_MONITOR_FORWARDED.inc();
            tracing::warn!(%status, what = %c.what, "awzy-pdp unreachable; install was monitoring, forwarding");
            None
        }
        Err(status) => outage(state, up, principal, tool, &c, id, &status),
    }
}

/// The PDP is unreachable and the install is enforcing: replay a recent allow for a listed
/// fail-open tool, deny everything else (ADR 0030). A resource check replays only an allow for
/// the same resource.
fn outage(
    state: &GatewayState,
    up: &UpstreamConfig,
    principal: &Principal,
    tool: &str,
    c: &ToolCheck<'_>,
    id: &Value,
    status: &tonic::Status,
) -> Option<Response<Body>> {
    let listed = state.0.fallback.outage_tier(&up.name, tool);
    let tier = listed.as_ref().map(|(t, _)| t.as_str());
    let fail_open = listed.as_ref().is_some_and(|(_, open)| *open);
    let who: Vec<&awzy_model::EntityRef> =
        std::iter::once(&principal.subject).chain(principal.on_behalf_of.as_ref()).collect();
    let containment = state.0.containments.replay(&who);
    let replay = match containment {
        Replay::Allowed if fail_open => state.0.allows.recent(&c.cache_key),
        _ => None,
    };
    match replay {
        Some(age) => {
            crate::metrics::OUTAGE_REPLAYED.inc();
            tracing::warn!(%status, what = %c.what, tier = ?tier, age_secs = age.as_secs(), "awzy-pdp unreachable; replaying a recent allow");
            None
        }
        None => {
            crate::metrics::OUTAGE_DENIED.inc();
            tracing::warn!(%status, what = %c.what, tier = ?tier, "awzy-pdp unreachable; denying");
            let why = match (tier, containment) {
                (None, _) => {
                    "it isn't classified by name in the client portal's outage settings, so it fails closed".to_string()
                }
                (Some(t), _) if !fail_open => format!("its tier ({t}) fails closed"),
                (Some(_), Replay::Contained) => "this caller's access is contained".to_string(),
                (Some(_), Replay::Unknown) => {
                    "the gateway hasn't yet learned who is contained, so it replays nothing".to_string()
                }
                (Some(_), Replay::Allowed) => "no recent allow for this caller could be replayed".to_string(),
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
fn blocked(id: &Value, what: &str, res: &CheckResponse, portal_url: Option<&str>) -> Response<Body> {
    let (text, meta) = blocked_message(what, res, portal_url);
    tool_error(id, text, meta)
}

/// What a blocked call's result says, and its `_meta`: the decision, and for a challenge, where
/// to answer it, so the person, or the agent working for them, can go straight there.
fn blocked_message(what: &str, res: &CheckResponse, portal_url: Option<&str>) -> (String, Value) {
    let challenge_url = portal_url
        .filter(|_| !res.challenge_qid.is_empty())
        .map(|u| format!("{}/challenges?challenge={}", u.trim_end_matches('/'), res.challenge_qid));
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
                let open = challenge_url.as_ref().map(|u| format!(" Answer it at {u}.")).unwrap_or_default();
                format!(
                    "The call was not made. Challenge {} is waiting for an answer;{open} Once it's passed, \
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
                "awzy denied {what} (decision {}; {policies}). If this access is needed, ask the human \
                 you're working for or a security admin.",
                res.decision_qid
            ),
        ),
    };
    let mut meta = json!({ "awzy/decision_qid": res.decision_qid, "awzy/verdict": verdict });
    if !res.challenge_qid.is_empty() {
        meta["awzy/challenge_qid"] = json!(res.challenge_qid);
    }
    if let Some(url) = challenge_url {
        meta["awzy/challenge_url"] = json!(url);
    }
    (text, meta)
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

/// A JSON-RPC parse error, with 400 as streamable HTTP allows, for a body that isn't forwarded.
fn unreadable() -> Response<Body> {
    let mut res =
        jsonrpc_error(&Value::Null, -32700, "awzy-gateway couldn't parse this message, so it wasn't forwarded");
    *res.status_mut() = StatusCode::BAD_REQUEST;
    res
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
mod tests;

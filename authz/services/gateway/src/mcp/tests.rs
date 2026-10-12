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
                if msg["method"] == "initialize" {
                    let result = json!({ "protocolVersion": "2025-06-18", "capabilities": {} });
                    return axum::response::IntoResponse::into_response((
                        [("mcp-session-id", "session-1")],
                        axum::Json(json!({ "jsonrpc": "2.0", "id": msg["id"], "result": result })),
                    ));
                }
                let result = match msg["params"]["name"].as_str() {
                    Some(tool) => {
                        seen.0.lock().unwrap().push(tool.to_string());
                        json!({ "content": [{ "type": "text", "text": format!("ran {tool}") }] })
                    }
                    None => json!({ "tools": [] }),
                };
                axum::response::IntoResponse::into_response(axum::Json(
                    json!({ "jsonrpc": "2.0", "id": msg["id"], "result": result }),
                ))
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
    let (app, down, _) = gateway_with_containments(mode, upstream_url, Arc::new(Containments::none())).await;
    (app, down)
}

/// The gateway with a PDP that can be switched off, and the containments it follows.
async fn gateway_with_containments(
    mode: awzy_model::EnforcementMode,
    upstream_url: String,
    containments: Arc<Containments>,
) -> (Router, Arc<AtomicBool>, GatewayState) {
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
        outage: crate::config::OutageConfig { cached_allow_ttl_secs: 300 },
        service_account_registry: crate::config::Registry::Config,
        public_url: Some("https://mcp.test".into()),
        portal_url: Some("https://portal.test".into()),
    };
    let state = GatewayState::with_containments(config, pdp, containments)
        .unwrap()
        .with_fallback(Arc::new(Fallback::fixed(drive_tiers())));
    (router(state.clone()), down, state)
}

/// What the client portal would set for the test's drive: three tools by name, the rest of
/// drive internal, and internal failing open.
fn drive_tiers() -> awzy_model::OutageFallback {
    let mut f = awzy_model::OutageFallback::default();
    for (tool, tier) in
        [("drive/delete_file", "confidential"), ("drive/read_file", "internal"), ("drive/read_document", "internal")]
    {
        f.tools.insert(tool.into(), tier.into());
    }
    f.servers.insert("drive".into(), "internal".into());
    f.fail_open_tiers.insert("internal".into());
    f
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

    assert_eq!(*seen.0.lock().unwrap(), vec!["read_file", "delete_file"], "bob's delete never reached the upstream");
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
async fn a_message_the_gateway_cant_parse_or_a_method_it_doesnt_check_never_reaches_the_upstream() {
    let seen = Upstream::default();
    let app = gateway(awzy_model::EnforcementMode::Enforce, start_upstream(seen.clone()).await).await;
    let bob = token("bob", None, &["engineering"]);
    let send = |method: Method, body: String| {
        let app = app.clone();
        let bob = bob.clone();
        async move {
            let req = Request::builder()
                .method(method)
                .uri("/mcp/drive")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {bob}"))
                .body(Body::from(body))
                .unwrap();
            app.oneshot(req).await.unwrap().status()
        }
    };
    // Deeper than serde_json's recursion limit, and a lone surrogate: both parse in other
    // JSON parsers, as a delete bob may not make.
    let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
    let nested = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"delete_file","arguments":{{"x":{deep}}}}}}}"#
    );
    let surrogate =
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"delete_file","arguments":{"x":"\ud800"}}}"#;
    assert_eq!(send(Method::POST, nested).await, StatusCode::BAD_REQUEST);
    assert_eq!(send(Method::POST, surrogate.to_string()).await, StatusCode::BAD_REQUEST);
    assert_eq!(send(Method::POST, "\"tools/call\"".into()).await, StatusCode::BAD_REQUEST);
    let put = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"delete_file"}}"#;
    assert_eq!(send(Method::PUT, put.into()).await, StatusCode::METHOD_NOT_ALLOWED);
    assert!(seen.0.lock().unwrap().is_empty(), "nothing reached the upstream");
}

#[tokio::test]
async fn a_session_is_known_by_the_client_its_initialize_named_for_whoever_started_it() {
    let (app, _, state) = gateway_with_containments(
        awzy_model::EnforcementMode::Enforce,
        start_upstream(Upstream::default()).await,
        Arc::new(Containments::none()),
    )
    .await;
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "claude-code\n", "version": "2" } } });
    let req = Request::post("/mcp/drive")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token("bob", None, &["engineering"])))
        .body(Body::from(init.to_string()))
        .unwrap();
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);

    let bob = state.0.verifier.principal(Some(&format!("Bearer {}", token("bob", None, &[]))), &Vec::new()).await;
    let alice = state.0.verifier.principal(Some(&format!("Bearer {}", token("alice", None, &[]))), &Vec::new()).await;
    assert_eq!(state.0.sessions.client("session-1", &bob.unwrap()).as_deref(), Some("claude-code"));
    assert_eq!(state.0.sessions.client("session-1", &alice.unwrap()), None, "someone else's session");
    assert_eq!(
        check_context(Some("claude-code")),
        json!({ "client": "mcp", "mcp_client": "claude-code" }).as_object().unwrap().clone()
    );
}

#[tokio::test]
async fn an_mcp_client_finds_where_to_sign_in() {
    let app = gateway(awzy_model::EnforcementMode::Enforce, start_upstream(Upstream::default()).await).await;
    let res = app.clone().oneshot(Request::post("/mcp/drive").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        res.headers()[header::WWW_AUTHENTICATE],
        r#"Bearer error="invalid_token", resource_metadata="https://mcp.test/.well-known/oauth-protected-resource/mcp/drive""#
    );
    let get = |path: &'static str| {
        let app = app.clone();
        async move { app.oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap() }
    };
    let res = get("/.well-known/oauth-protected-resource/mcp/drive").await;
    let metadata: Value =
        serde_json::from_slice(&axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(metadata["resource"], "https://mcp.test/mcp/drive");
    assert_eq!(metadata["authorization_servers"], json!(["https://idp.test"]));
    assert_eq!(get("/.well-known/oauth-protected-resource/mcp/nope").await.status(), StatusCode::NOT_FOUND);
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
    assert_eq!(replayed["result"]["content"][0]["text"], "ran read_file", "internal fails open with a recent allow");

    let (_, confidential) = call(&app, Some(&alice), "delete_file").await;
    assert_eq!(confidential["result"]["_meta"]["awzy/verdict"], "unavailable");
    assert!(confidential["result"]["content"][0]["text"].as_str().unwrap().contains("(confidential) fails closed"));

    let (_, unlisted) = call(&app, Some(&alice), "list_files").await;
    assert!(
        unlisted["result"]["content"][0]["text"].as_str().unwrap().contains("isn't classified by name"),
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

/// The drill from the platform plan's Phase 3: an outage never replays an allow for an
/// principal contained before it.
#[tokio::test]
async fn an_outage_never_replays_an_allow_for_someone_contained_before_it() {
    let seen = Upstream::default();
    let containments = Arc::new(Containments::awaiting_feed());
    let (app, pdp_down, state) = gateway_with_containments(
        awzy_model::EnforcementMode::Enforce,
        start_upstream(seen.clone()).await,
        containments.clone(),
    )
    .await;
    let alice = token("alice", None, &["security"]);
    let carol = token("carol", None, &["security"]);
    // An agent acting for alice: contained along with her.
    let genie_for_alice = token("alice", Some("sales-genie"), &["security"]);

    // Until the first list arrives, nothing replays.
    call(&app, Some(&alice), "read_file").await;
    pdp_down.store(true, Ordering::Release);
    let (_, unknown) = call(&app, Some(&alice), "read_file").await;
    assert!(unknown["result"]["content"][0]["text"].as_str().unwrap().contains("hasn't yet learned"), "{unknown}");
    pdp_down.store(false, Ordering::Release);

    // The feed arrives with no one contained; everyone builds up recent allows.
    let list = |who: &[&str]| who.iter().map(|w| format!("Human::{w}")).collect::<std::collections::HashSet<_>>();
    state.forget_allows(&list(&[]));
    containments.load(list(&[]));
    for t in [&alice, &carol, &genie_for_alice] {
        let (_, ok) = call(&app, Some(t), "read_file").await;
        assert_eq!(ok["result"]["content"][0]["text"], "ran read_file");
    }

    // Alice is contained, then the PDP goes down.
    state.forget_allows(&list(&["alice"]));
    containments.load(list(&["alice"]));
    pdp_down.store(true, Ordering::Release);
    for t in [&alice, &genie_for_alice] {
        let (_, refused) = call(&app, Some(t), "read_file").await;
        assert!(refused["result"]["content"][0]["text"].as_str().unwrap().contains("contained"), "{refused}");
    }
    let (_, carol_replayed) = call(&app, Some(&carol), "read_file").await;
    assert_eq!(carol_replayed["result"]["content"][0]["text"], "ran read_file", "carol isn't contained");

    // Lifted while the PDP is still down: her old allows were forgotten, so still nothing.
    containments.load(list(&[]));
    let (_, lifted) = call(&app, Some(&alice), "read_file").await;
    assert!(lifted["result"]["content"][0]["text"].as_str().unwrap().contains("no recent allow"), "{lifted}");

    assert_eq!(seen.0.lock().unwrap().len(), 5, "1 before the feed, 3 with the PDP up, and carol's replay");
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
fn a_challenged_call_links_to_the_challenge_in_the_portal() {
    let res = CheckResponse {
        decision_qid: "decision_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e90".into(),
        challenge_qid: "challenge_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e91".into(),
        verdict: Verdict::Challenge as i32,
        ..Default::default()
    };
    let url = "https://portal.test/challenges?challenge=challenge_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e91";
    let (text, meta) = blocked_message("drive/read_document", &res, Some("https://portal.test/"));
    assert!(text.contains(&format!("Answer it at {url}.")), "{text}");
    assert_eq!(meta["awzy/challenge_url"], url);
    assert_eq!(
        meta["awzy/challenge_wait"], "/mcp/challenges/challenge_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e91?wait=25",
        "an agent with no person at hand waits there"
    );
    let (text, meta) = blocked_message("drive/read_document", &res, None);
    assert!(!text.contains("Answer it at") && meta.get("awzy/challenge_url").is_none(), "no portal, no link");
    let no_challenge = CheckResponse { challenge_qid: String::new(), ..res };
    let (_, meta) = blocked_message("drive/read_document", &no_challenge, Some("https://portal.test"));
    assert!(
        meta.get("awzy/challenge_url").is_none() && meta.get("awzy/challenge_wait").is_none(),
        "nothing to link to"
    );
}

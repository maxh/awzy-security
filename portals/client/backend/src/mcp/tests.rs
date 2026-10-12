use axum::{body::Body, http::Request};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::*;
use crate::{
    policies::tests::{serve_fake_policy, FakePolicy},
    testing,
};

async fn services() -> Services {
    Services {
        policy: Some(serve_fake_policy(FakePolicy::default()).await),
        pdp: Some(testing::pdp().await),
        ..Default::default()
    }
}

async fn server() -> ClientPortalMcp {
    ClientPortalMcp { services: services().await }
}

/// Request parts as the sign-in middleware leaves them for `viewer`.
fn signed_in(viewer: Viewer) -> Extension<Parts> {
    let (mut parts, ()) = Request::new(()).into_parts();
    parts.extensions.insert(viewer);
    Extension(parts)
}

fn alice() -> Extension<Parts> {
    signed_in(testing::human("alice", "security"))
}

#[test]
fn a_principal_comes_with_every_group_above_it_and_its_risk() {
    let snapshot = serde_json::json!({
        "entities": [
            { "uid": { "type": "Human", "id": "bob" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "appsec" }] },
            { "uid": { "type": "Group", "id": "appsec" }, "attrs": {}, "parents": [{ "type": "Group", "id": "security" }] },
            { "uid": { "type": "Group", "id": "security" }, "attrs": {}, "parents": [] },
        ],
        "risk": { "Human::bob": 42 },
    });
    let bob = principal_from_snapshot(&snapshot, "Human", "bob");
    assert_eq!((bob.groups, bob.risk_score), (vec!["appsec".to_string(), "security".to_string()], Some(42)));
    assert_eq!(bob.entity.unwrap()["attrs"]["active"], true);
    let nobody = principal_from_snapshot(&snapshot, "Human", "nobody");
    assert!(nobody.entity.is_none() && nobody.groups.is_empty() && nobody.risk_score.is_none());
}

#[tokio::test]
async fn check_access_decides_without_recording_or_challenging() {
    let server = server().await;
    let ask = |principal_type: &str, principal_id: &str, on_behalf_of_id: Option<&str>| CheckAccessParams {
        principal_type: principal_type.into(),
        principal_id: principal_id.into(),
        action: "view".into(),
        resource_type: "PortalArea".into(),
        resource_id: "challenges".into(),
        on_behalf_of_id: on_behalf_of_id.map(Into::into),
        human_present: None,
    };
    let Json(bob) = server.check_access(alice(), Parameters(ask("Human", "bob", None))).await.unwrap();
    assert_eq!((bob.verdict.as_str(), bob.policies), ("allow", vec!["humans-answer-their-challenges".to_string()]));
    let Json(agent) =
        server.check_access(alice(), Parameters(ask("ServiceAccount", "sales-genie", Some("bob")))).await.unwrap();
    assert_eq!(agent.verdict, "deny", "no policy lets an agent view challenges");
    assert_eq!(agent.on_behalf_of.unwrap().verdict, "allow", "bob could");

    // Only those who may view policies may ask.
    let bob = signed_in(testing::human("bob", "engineering"));
    let Err(err) = server.check_access(bob, Parameters(ask("Human", "bob", None))).await else {
        panic!("bob may not view policies")
    };
    assert!(err.message.contains("may not view policies"), "{}", err.message);
}

#[tokio::test]
async fn propose_policy_only_creates_drafts() {
    let mcp = server().await;
    assert!(mcp.get_active_policy_bundle(alice()).await.unwrap().0.bundle.is_none());

    let bad = mcp
        .propose_policy(alice(), Parameters(ProposePolicyParams { policies: "permit (".into(), schema: None }))
        .await
        .unwrap()
        .0;
    assert!(bad.draft.is_none() && bad.validation_error.is_some());

    let good = mcp
        .propose_policy(
            alice(),
            Parameters(ProposePolicyParams { policies: awzy_pdp::DEFAULT_POLICIES.into(), schema: None }),
        )
        .await
        .unwrap()
        .0;
    let draft = good.draft.unwrap();
    assert_eq!(draft.status, "draft");
    assert_eq!(draft.created_by, "Human::alice");
    assert!(draft.id.starts_with("policy_bundle_"));
    assert!(mcp.get_active_policy_bundle(alice()).await.unwrap().0.bundle.is_none(), "proposing never publishes");

    let bob = signed_in(testing::human("bob", "engineering"));
    let Err(err) =
        mcp.propose_policy(bob, Parameters(ProposePolicyParams { policies: String::new(), schema: None })).await
    else {
        panic!("bob isn't in the security group");
    };
    assert!(err.message.starts_with("bob may not administer policies"), "{}", err.message);
}

#[test]
fn explains_a_delegated_denial_by_the_persons_check() {
    let row = DecisionRow {
        id: uuid::Uuid::now_v7(),
        at: chrono::Utc::now(),
        principal_type: "ServiceAccount".into(),
        principal_id: "ops-bot".into(),
        on_behalf_of_type: "Human".into(),
        on_behalf_of_id: "bob".into(),
        human_present: true,
        action: "read".into(),
        resource_type: "Resource".into(),
        resource_id: "board-deck".into(),
        verdict: "deny".into(),
        mode: "enforce".into(),
        challenge: String::new(),
        policies: vec!["security-team-read".into()],
        on_behalf_of_verdict: "deny".into(),
        on_behalf_of_policies: vec![],
        policy_version: 0,
        context: "{}".into(),
        resource_attributes_from: "awzy-context".into(),
    };
    let own = vec![decisions::PolicySource { id: "security-team-read".into(), source: String::new() }];
    let s = explain_sentence(&row, &own);
    assert!(
        s.starts_with("Denied: Human::bob couldn't do this themselves, even though the service account could"),
        "{s}"
    );
    assert_eq!(summary_of(&row).on_behalf_of.as_deref(), Some("Human::bob"));
}

#[tokio::test]
async fn explain_rejects_ids_that_are_not_decisions() {
    let mcp = server().await;
    let Err(err) = mcp
        .explain_decision(alice(), Parameters(ExplainDecisionParams { decision_qid: "policy_bundle_0199".into() }))
        .await
    else {
        panic!("a policy bundle ID must not be accepted as a decision ID");
    };
    assert!(err.message.contains("is not a decision ID"), "{}", err.message);
}

const SECRET_ENV: &str = "AWZY_TEST_PORTAL_SECRET";

fn token_config(audience: &str) -> awzy_identity::TokenConfig {
    std::env::set_var(SECRET_ENV, "test-secret");
    awzy_identity::TokenConfig {
        issuer: "https://idp.test".into(),
        audience: audience.into(),
        jwks_ca_file: None,
        subject_claim: None,
        key: awzy_identity::TokenKey::Hs256SecretEnv(SECRET_ENV.into()),
    }
}

fn agents() -> Vec<awzy_identity::ServiceAccountConfig> {
    vec![awzy_identity::ServiceAccountConfig {
        id: "sec-agent".into(),
        owner: "alice".into(),
        ai: true,
        groups: vec!["security".into()],
    }]
}

/// The portal router, with an HS256 test key: tokens for `awzy` on GraphQL, `awzy-mcp` on MCP.
async fn app() -> Router {
    let portal = Some("https://portal.test".to_string());
    let auth = access::Auth::new(token_config("awzy"), agents(), "portal".into(), portal.clone()).unwrap();
    let mcp = access::Auth::new_for_mcp(token_config("awzy-mcp"), agents(), "portal".into(), portal).unwrap();
    crate::router(services().await, auth, mcp, vec![], awzy_portal::PersistedQueries::none())
}

fn bearer(claims: Value) -> String {
    bearer_for("awzy-mcp", claims)
}

fn bearer_for(audience: &str, claims: Value) -> String {
    let mut claims = claims;
    claims.as_object_mut().unwrap().extend(
        json!({ "iss": "https://idp.test", "aud": audience, "exp": 4_000_000_000_u64 }).as_object().unwrap().clone(),
    );
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap();
    format!("Bearer {token}")
}

/// One JSON-RPC exchange with the /mcp endpoint, as the agent acting for alice. Returns the
/// response body and session ID.
async fn rpc(app: &Router, body: Value, session: Option<&str>) -> (Value, Option<String>) {
    let token = bearer(json!({ "sub": "alice", "groups": ["security"], "act": { "sub": "sec-agent" } }));
    let mut req = Request::post("/mcp")
        .header("authorization", token)
        .header("host", "localhost")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-06-18");
    if let Some(s) = session {
        req = req.header("mcp-session-id", s);
    }
    let res = app.clone().oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = res.status();
    let session = res.headers().get("mcp-session-id").map(|v| v.to_str().unwrap().to_string());
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(status.is_success(), "{status}: {text}");
    // JSON, or an SSE stream whose data lines carry the JSON-RPC message.
    let json = text
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or(text.trim());
    (if json.is_empty() { Value::Null } else { serde_json::from_str(json).unwrap() }, session)
}

#[tokio::test]
async fn speaks_mcp_over_http_on_the_portal_router() {
    let app = app().await;

    let anonymous = app
        .clone()
        .oneshot(Request::post("/mcp").header("host", "localhost").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(anonymous.status(), 401);
    assert_eq!(
        anonymous.headers()["www-authenticate"],
        r#"Bearer error="invalid_token", resource_metadata="https://portal.test/.well-known/oauth-protected-resource/mcp""#
    );

    let (init, session) = rpc(
        &app,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } } }),
        None,
    )
    .await;
    assert!(init["result"]["capabilities"]["tools"].is_object(), "{init}");
    rpc(&app, json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }), session.as_deref()).await;

    let (tools, _) = rpc(&app, json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }), session.as_deref()).await;
    let mut names: Vec<&str> =
        tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "check_access",
            "dry_run_policy",
            "explain_decision",
            "get_active_policy_bundle",
            "get_enforcement",
            "get_investigation",
            "get_principal",
            "get_resource",
            "list_investigations",
            "list_policy_bundles",
            "principal_activity",
            "propose_policy",
            "request_containment_lift",
            "search_decisions"
        ]
    );

    let (call, _) = rpc(
        &app,
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": { "name": "propose_policy", "arguments": { "policies": awzy_pdp::DEFAULT_POLICIES } } }),
        session.as_deref(),
    )
    .await;
    assert_eq!(call["result"]["structuredContent"]["draft"]["status"], "draft", "{call}");
    assert_eq!(
        call["result"]["structuredContent"]["draft"]["created_by"], "ServiceAccount::sec-agent for Human::alice",
        "{call}"
    );
}

#[tokio::test]
async fn a_token_for_mcp_doesnt_open_graphql_or_the_reverse() {
    let app = app().await;
    let alice = json!({ "sub": "alice", "groups": ["security"] });
    let status = |path: &'static str, token: String| {
        let app = app.clone();
        async move {
            let req = Request::post(path)
                .header("authorization", token)
                .header("host", "localhost")
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .body(Body::from(r#"{"query":"{ apiVersion }"}"#))
                .unwrap();
            app.oneshot(req).await.unwrap().status()
        }
    };
    assert_eq!(status("/graphql", bearer_for("awzy-mcp", alice.clone())).await, 401, "an agent's MCP token");
    assert_eq!(status("/mcp", bearer_for("awzy", alice.clone())).await, 401, "the portal's own token");
    assert_eq!(status("/graphql", bearer_for("awzy", alice)).await, 200);

    let metadata = app
        .oneshot(Request::get("/.well-known/oauth-protected-resource/mcp").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(metadata.into_body(), usize::MAX).await.unwrap();
    let metadata: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(metadata["resource"], "https://portal.test/mcp");
    assert_eq!(metadata["authorization_servers"], json!(["https://idp.test"]));
}

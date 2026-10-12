use std::{str::FromStr, sync::Arc};

use awzy_authz_proto::{authz::v1::ChallengeKind as WireKind, challenge::v1::ResolveRequest};
use awzy_challenge::{approvals::Approvals, ChallengeStore, Feed, Lifetimes};
use serde_json::json;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use tonic::transport::Channel;

use super::*;
use crate::testing::{human, pdp, pdp_endpoint, run, run_err};

/// The schema with a PDP and an awzy-challenge in a fresh Postgres schema.
async fn setup() -> Option<(crate::ClientPortalSchema, ChallengeClient)> {
    let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
        eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
        return None;
    };
    let schema = format!("test_{}", uuid::Uuid::now_v7().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&PgPool::connect(&url).await.unwrap()).await.unwrap();
    let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new().max_connections(2).connect_with(options).await.unwrap();
    let store = Arc::new(ChallengeStore::new(pool, Lifetimes::default()));
    store.migrate().await.unwrap();
    let keys = Arc::new(awzy_challenge::keys::SecurityKeys::new(
        store.pool().clone(),
        awzy_challenge::webauthn::RelyingParty::new("portal.example.com", ["https://portal.example.com"]),
        store.clone(),
    ));
    let app = awzy_challenge::router(
        store,
        Arc::new(Feed::default()),
        Arc::new(Approvals::pdp(&pdp_endpoint().await).unwrap()),
        Some(keys),
        &awzy_challenge::Access::off(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let channel = Channel::from_shared(format!("http://{addr}")).unwrap().connect().await.unwrap();
    let challenge = ChallengeClient::new(awzy_service_auth::grpc_channel(channel, "awzy-challenge"));
    let services = Services { pdp: Some(pdp().await), challenge: Some(challenge.clone()), ..Default::default() };
    Some((crate::schema_with(services), challenge))
}

/// Opens a challenge for `who` reading the board deck, as a PDP would.
async fn open(client: &mut ChallengeClient, who: &str, kind: WireKind) -> String {
    let req = ResolveRequest {
        decision_qid: awzy_model::DecisionId::new().to_string(),
        principal: Some(awzy_authz_proto::authz::v1::EntityRef { r#type: "Human".into(), id: who.into() }),
        on_behalf_of: None,
        action: "read".into(),
        resource: Some(awzy_authz_proto::authz::v1::EntityRef { r#type: "Resource".into(), id: "board-deck".into() }),
        kinds: vec![kind.into()],
        answerer_id: who.into(),
        open: true,
        ..Default::default()
    };
    client.resolve(req).await.unwrap().into_inner().challenge_qid
}

#[tokio::test]
async fn people_see_and_answer_only_their_own_challenges() {
    let Some((schema, mut client)) = setup().await else { return };
    let id = open(&mut client, "bob", WireKind::Justification).await;
    let (bob, carol, alice) = (human("bob", "engineering"), human("carol", "engineering"), human("alice", "security"));

    let mine = run(&schema, &bob, "{ myChallenges(status: PENDING) { nodes { id kinds status } } }").await;
    assert_eq!(mine["myChallenges"]["nodes"], json!([{ "id": id, "kinds": ["JUSTIFICATION"], "status": "PENDING" }]));
    let theirs = run(&schema, &carol, "{ myChallenges { nodes { id } } }").await;
    assert_eq!(theirs["myChallenges"]["nodes"], json!([]));
    let node = format!(r#"{{ node(id: "{id}") {{ id }} }}"#);
    assert!(run_err(&schema, &carol, node.as_str()).await.contains("may not administer challenges"));
    assert_eq!(run(&schema, &alice, node.as_str()).await["node"]["id"], json!(id), "an admin sees any");

    let justify = |text: &str| {
        format!(
            r#"mutation {{ justifyChallenge(input: {{ challengeQid: "{id}", justification: "{text}" }}) {{ challenge {{ status justification }} }} }}"#
        )
    };
    let why = "Preparing the audit committee's Q3 review";
    assert!(run_err(&schema, &carol, justify(why)).await.contains("only bob can answer"));
    assert!(run_err(&schema, &bob, justify("because")).await.contains("at least 20 characters"));
    let done = run(&schema, &bob, justify(why)).await;
    assert_eq!(done["justifyChallenge"]["challenge"], json!({ "status": "PASSED", "justification": why }));
}

#[tokio::test]
async fn approvals_and_containments_are_for_those_policy_allows() {
    let Some((schema, mut client)) = setup().await else { return };
    let id = open(&mut client, "bob", WireKind::Approval).await;
    let (bob, carol, alice) = (human("bob", "engineering"), human("carol", "engineering"), human("alice", "security"));

    assert!(run_err(&schema, &bob, "{ challenges { nodes { id } } }").await.contains("may not administer challenges"));
    // Each approver sees what policy lets them approve, and nobody their own request.
    let to_approve = |v: &Viewer| {
        let (schema, v) = (schema.clone(), v.clone());
        async move {
            run(&schema, &v, "{ challengesToApprove { nodes { id } } }").await["challengesToApprove"]["nodes"].clone()
        }
    };
    assert_eq!(to_approve(&alice).await, json!([{ "id": id }]));
    assert_eq!(to_approve(&bob).await, json!([]), "bob asked");
    assert_eq!(to_approve(&carol).await, json!([]), "policy doesn't let carol approve the board deck");
    let pending = run(&schema, &alice, "{ challenges(status: PENDING) { nodes { id principal { id } } } }").await;
    assert_eq!(pending["challenges"]["nodes"], json!([{ "id": id, "principal": { "id": "bob" } }]));

    let decide = format!(
        r#"mutation {{ decideChallenge(input: {{ challengeQid: "{id}", approve: true, note: "ok" }}) {{ challenge {{ status decidedBy {{ id }} }} }} }}"#
    );
    assert!(run_err(&schema, &bob, decide.as_str()).await.contains("doesn't let bob approve"));
    assert!(run_err(&schema, &carol, decide.as_str()).await.contains("doesn't let carol approve"));
    let approved = run(&schema, &alice, decide.as_str()).await;
    assert_eq!(approved["decideChallenge"]["challenge"], json!({ "status": "PASSED", "decidedBy": { "id": "alice" } }));

    let contain = r#"mutation { containPrincipal(input: { principal: { type: "ServiceAccount", id: "sales-genie" },
        reason: "bulk export of the pipeline" }) { containment { id createdBy { type id } liftedAt } } }"#;
    assert!(run_err(&schema, &bob, contain).await.contains("may not administer challenges"));
    let c = run(&schema, &alice, contain).await["containPrincipal"]["containment"].clone();
    assert_eq!((&c["createdBy"], &c["liftedAt"]), (&json!({ "type": "Human", "id": "alice" }), &json!(null)));
    let active = run(&schema, &alice, "{ containments { nodes { id principal { type id } } } }").await;
    assert_eq!(
        active["containments"]["nodes"],
        json!([{ "id": c["id"], "principal": { "type": "ServiceAccount", "id": "sales-genie" } }])
    );

    let lift = format!(
        r#"mutation {{ liftContainment(input: {{ containmentQid: {}, reason: "reviewed" }}) {{ containment {{ liftedBy {{ id }} }} }} }}"#,
        c["id"]
    );
    assert_eq!(run(&schema, &alice, lift).await["liftContainment"]["containment"]["liftedBy"]["id"], json!("alice"));
    assert_eq!(run(&schema, &alice, "{ containments { nodes { id } } }").await["containments"]["nodes"], json!([]));
}

#[tokio::test]
async fn security_keys_start_with_the_portals_relying_party() {
    let Some((schema, mut client)) = setup().await else { return };
    let bob = human("bob", "engineering");
    assert_eq!(run(&schema, &bob, "{ mySecurityKeys { id } }").await["mySecurityKeys"], json!([]));

    let begin =
        run(&schema, &bob, "mutation { beginSecurityKeyRegistration(input: {}) { ceremonyQid optionsJson } }").await;
    let c = &begin["beginSecurityKeyRegistration"];
    assert!(c["ceremonyQid"].as_str().unwrap().starts_with("ceremony_"));
    let options: serde_json::Value = serde_json::from_str(c["optionsJson"].as_str().unwrap()).unwrap();
    assert_eq!(options["rp"]["id"], "portal.example.com");
    assert_eq!(options["pubKeyCredParams"], json!([{ "type": "public-key", "alg": -7 }]));

    // Without a key, there's nothing to verify with, and nothing to authorize a change.
    let id = open(&mut client, "bob", WireKind::Verification).await;
    let verify =
        format!(r#"mutation {{ beginChallengeVerification(input: {{ challengeQid: "{id}" }}) {{ ceremonyQid }} }}"#);
    assert!(run_err(&schema, &bob, verify.as_str()).await.contains("has no security key"));
    let change = "mutation { beginSecurityKeyChange { ceremonyQid } }";
    assert!(run_err(&schema, &bob, change).await.contains("has no security key"));
    // Someone else can't start a verification of bob's challenge.
    let carol = human("carol", "engineering");
    assert!(run_err(&schema, &carol, verify.as_str()).await.contains("only bob can answer"));
}

#[tokio::test]
async fn a_subscription_ends_when_the_token_it_started_with_does() {
    let mut bob = awzy_identity::Principal::human("bob", None, &[]);
    bob.expires_at = Some(SystemTime::now() - Duration::from_secs(1));
    let forever = async_graphql::futures_util::stream::pending::<()>();
    let stream = while_allowed(Services::default(), Viewer(Arc::new(bob), None), PortalAction::View, forever);
    let sent = tokio::time::timeout(Duration::from_secs(5), stream.collect::<Vec<_>>()).await;
    assert_eq!(sent.expect("ended at once"), Vec::<()>::new());
}

#[test]
fn only_a_person_answers() {
    let mut agent = awzy_identity::Principal::human("bob", None, &[]);
    agent.subject = awzy_model::EntityRef::new("ServiceAccount", "sales-genie");
    agent.on_behalf_of = Some(awzy_model::EntityRef::new("Human", "bob"));
    let err = answering_human(&Viewer(Arc::new(agent), None)).unwrap_err();
    assert!(err.message.contains("sales-genie for bob can't answer challenges"), "{}", err.message);
    assert_eq!(answering_human(&crate::testing::human("bob", "engineering")).unwrap(), "bob");
}

use std::time::Duration;

use awzy_authz_proto::challenge::v1::challenge_service_client::ChallengeServiceClient;
use awzy_model::{Check, EnforcementMode, Verdict};
use awzy_pdp::{challenges::ChallengeResolver, containments::Containments, AppState, Enforcement, Pdp};
use serde_json::json;
use tonic::{transport::Channel, Code};

use super::*;
use crate::store::{tests::test_store, Lifetimes};

const POLICIES: &str = r#"
    @id("anyone-reads")
    permit (principal, action == Action::"read", resource);
    @id("crown-jewels-need-a-reason")
    @challenge("justification")
    forbid (principal, action == Action::"read", resource) when { resource.sensitivity == "crown_jewels" };
    @id("confidential-needs-approval")
    @challenge("approval")
    forbid (principal, action == Action::"read", resource) when { resource.sensitivity == "confidential" };
"#;

async fn serve(store: ChallengeStore) -> (String, ChallengeServiceClient<Channel>) {
    let store = Arc::new(store);
    let feed = Arc::new(Feed::default());
    tokio::spawn(feed.clone().follow(store.clone()));
    let approvals = Arc::new(Approvals::Only(vec!["carol".into(), "overseer".into()]));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let app = router(store, feed, approvals, None, &Access::off());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = ChallengeServiceClient::connect(endpoint.clone()).await.unwrap();
    (endpoint, client)
}

async fn pdp(endpoint: &str) -> AppState {
    let containments = Arc::new(Containments::from_challenge(endpoint).unwrap());
    tokio::spawn(containments.clone().follow());
    let state = AppState::new(awzy_pdp::shared(Pdp::new(awzy_pdp::DEFAULT_SCHEMA, POLICIES).unwrap()))
        .with_enforcement(Arc::new(Enforcement::fixed(EnforcementMode::Enforce)))
        .with_challenges(containments, Arc::new(ChallengeResolver::from_challenge(endpoint).unwrap()));
    until(|| async { state.is_ready() }).await;
    state
}

/// Polls `ok` for up to 3 seconds, past the PDP's 2-second staleness window.
async fn until<F: std::future::Future<Output = bool>>(ok: impl Fn() -> F) {
    for _ in 0..60 {
        if ok().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("condition not met within 3 seconds");
}

fn read(who: &str, what: &str) -> (Check, serde_json::Value) {
    let sensitivity = match what {
        "board-deck" => "crown_jewels",
        "payroll" => "confidential",
        _ => "internal",
    };
    let req = Check {
        principal: awzy_model::EntityRef::new("Human", who),
        action: "read".into(),
        resource: awzy_model::EntityRef::new("Resource", what),
        context: Default::default(),
        on_behalf_of: None,
        human_present: None,
        always_enforced: false,
        work_item_id: None,
    };
    let entities = json!([
        { "uid": { "type": "Human", "id": who }, "attrs": {}, "parents": [] },
        { "uid": { "type": "Resource", "id": what }, "attrs": { "sensitivity": sensitivity }, "parents": [] },
    ]);
    (req, entities)
}

async fn check(state: &AppState, who: &str, what: &str) -> awzy_model::Decision {
    let (req, entities) = read(who, what);
    state.check(&req, entities).await.unwrap()
}

#[tokio::test]
async fn a_justified_request_is_allowed_on_retry() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let (endpoint, mut client) = serve(store).await;
    let state = pdp(&endpoint).await;

    let first = check(&state, "alice", "board-deck").await;
    assert!(matches!(first.verdict, Verdict::Challenge { .. }));
    let challenge_id = first.challenge_id.expect("an enforced challenge carries one to answer");
    assert_eq!(check(&state, "alice", "board-deck").await.challenge_id, Some(challenge_id), "retries share it");

    let challenge = client
        .justify(JustifyRequest {
            challenge_qid: challenge_id.to_string(),
            answered_by_actor: Some(awzy_model::Actor::human("alice").into()),
            justification: "Preparing the audit committee's review of Q3".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .challenge
        .unwrap();
    assert_eq!(challenge.status(), WireStatus::Passed);

    let retry = check(&state, "alice", "board-deck").await;
    assert_eq!((retry.verdict, retry.challenge_id), (Verdict::Allow, None));
    assert_eq!(retry.policies, ["anyone-reads"]);
    assert!(matches!(check(&state, "bob", "board-deck").await.verdict, Verdict::Challenge { .. }), "alice's only");
}

#[tokio::test]
async fn a_watcher_sees_its_challenges_as_they_open_and_no_one_elses() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let (endpoint, client) = serve(store).await;
    let state = pdp(&endpoint).await;
    let watch = |answerer: &str| {
        let mut client = client.clone();
        let req = WatchChallengesRequest { answerer_id: answerer.into() };
        async move { client.watch_challenges(req).await.unwrap().into_inner() }
    };
    let (mut bobs, mut erins) = (watch("bob").await, watch("erin").await);

    let id = check(&state, "bob", "payroll").await.challenge_id.unwrap().to_string();
    let seen = tokio::time::timeout(Duration::from_secs(5), bobs.message()).await.expect("streamed").unwrap().unwrap();
    assert_eq!(seen.challenge.unwrap().qid, id);
    let quiet = tokio::time::timeout(Duration::from_millis(500), erins.message()).await;
    assert!(quiet.is_err(), "erin's watch doesn't see bob's challenge");
}

#[tokio::test]
async fn approval_comes_from_someone_policy_allows() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let (endpoint, mut client) = serve(store).await;
    let state = pdp(&endpoint).await;

    let id = check(&state, "bob", "payroll").await.challenge_id.unwrap().to_string();
    let decide = |by: &str, approve: bool| DecideRequest {
        challenge_qid: id.clone(),
        decided_by: by.into(),
        approve,
        note: String::new(),
        approver_entities: vec![],
        ..Default::default()
    };
    let refused = client.decide(decide("dave", true)).await.unwrap_err();
    assert_eq!(refused.code(), Code::PermissionDenied);
    client.decide(decide("carol", true)).await.unwrap();
    assert_eq!(check(&state, "bob", "payroll").await.verdict, Verdict::Allow);

    let id = check(&state, "erin", "payroll").await.challenge_id.unwrap().to_string();
    client
        .decide(DecideRequest {
            challenge_qid: id,
            decided_by: "carol".into(),
            approve: false,
            note: "no".into(),
            approver_entities: vec![],
            ..Default::default()
        })
        .await
        .unwrap();
    let rejected = check(&state, "erin", "payroll").await;
    assert_eq!(rejected.verdict, Verdict::Deny);
    assert!(rejected.policies.contains(&awzy_pdp::challenges::CHALLENGE_REJECTED.to_string()));
}

#[tokio::test]
async fn an_overseer_agent_approves_on_its_own_authority() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let (endpoint, mut client) = serve(store).await;
    let state = pdp(&endpoint).await;

    let id = check(&state, "bob", "payroll").await.challenge_id.unwrap().to_string();
    let overseer = awzy_model::EntityRef::new(awzy_model::entity_type::SERVICE_ACCOUNT, "overseer");
    let decide = |actor: awzy_model::Actor| DecideRequest {
        challenge_qid: id.clone(),
        decided_by_actor: Some(actor.into()),
        approve: true,
        ..Default::default()
    };
    let for_someone = client.decide(decide(awzy_model::Actor::new(overseer.clone(), None).for_human("carol"))).await;
    assert_eq!(for_someone.unwrap_err().code(), Code::InvalidArgument, "approving is on its own authority");

    let c = client.decide(decide(awzy_model::Actor::new(overseer, None))).await.unwrap().into_inner();
    let by = c.challenge.unwrap().decided_by_actor.unwrap();
    assert_eq!(by.subject.unwrap().r#type, "ServiceAccount");
    assert_eq!(check(&state, "bob", "payroll").await.verdict, Verdict::Allow);
}

#[tokio::test]
async fn a_containment_reaches_the_pdp_within_its_freshness_target() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let (endpoint, mut client) = serve(store).await;
    let state = pdp(&endpoint).await;
    assert_eq!(check(&state, "mallory", "design-doc").await.verdict, Verdict::Allow);

    let started = std::time::Instant::now();
    let containment = client
        .contain(ContainRequest {
            principal: Some(awzy_model::EntityRef::new("Human", "mallory").into()),
            reason: "bulk download of design docs".into(),
            created_by_actor: Some(awzy_model::Actor::awzy("awzy-reasoner").into()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .containment
        .unwrap();
    assert_eq!(containment.created_by, "AwzyService::awzy-reasoner", "the typed actor, as text for older callers");
    until(|| async { check(&state, "mallory", "design-doc").await.verdict == Verdict::Deny }).await;
    assert!(started.elapsed() < Duration::from_secs(2), "took {:?}", started.elapsed());
    assert_eq!(check(&state, "mallory", "design-doc").await.policies, std::slice::from_ref(&containment.qid));
    assert_eq!(check(&state, "alice", "design-doc").await.verdict, Verdict::Allow, "only mallory");

    client
        .lift(LiftRequest {
            containment_qid: containment.qid,
            lifted_by: "carol".into(),
            reason: "reviewed".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    until(|| async { check(&state, "mallory", "design-doc").await.verdict == Verdict::Allow }).await;
}

#[tokio::test]
async fn a_monitored_containment_is_listed_but_never_applied() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let (endpoint, mut client) = serve(store).await;
    let state = pdp(&endpoint).await;
    let would = client
        .contain(ContainRequest {
            principal: Some(awzy_model::EntityRef::new("Human", "mallory").into()),
            reason: "rejected challenge and an export".into(),
            created_by_actor: Some(awzy_model::Actor::awzy("awzy-reasoner").into()),
            monitor: true,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .containment
        .unwrap();
    assert!(would.monitor);
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(check(&state, "mallory", "design-doc").await.verdict, Verdict::Allow, "monitored: not applied");

    let list = |include_monitor| ListContainmentsRequest { active_only: true, include_monitor, ..Default::default() };
    let listed = |r: tonic::Response<ListContainmentsResponse>| r.into_inner().containments.len();
    assert_eq!(listed(client.list_containments(list(false)).await.unwrap()), 0, "left out unless asked for");
    assert_eq!(listed(client.list_containments(list(true)).await.unwrap()), 1);
}

#[test]
fn every_method_the_service_has_has_exactly_one_access_rule() {
    // A method no rule names is refused for every caller, as awzy-policy's ListEnforcementSettings
    // once was.
    let proto = include_str!("../../../../libs/proto/awzy/challenge/v1/challenge.proto");
    let gaps = Access::off().guard().gaps(proto);
    assert!(gaps.is_empty(), "{gaps:#?}");
}

/// What a subscriber writes, kept for the test to read back.
#[derive(Clone, Default)]
struct Captured(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;

    fn make_writer(&'a self) -> Captured {
        self.clone()
    }
}

/// The payload field names `events.p` declares for `event`, without `at`, which the replay fills
/// from the line's time.
fn declared_fields(events: &str, event: &str) -> Vec<String> {
    let decl = events
        .lines()
        .find(|l| l.starts_with(&format!("event {event}:")))
        .unwrap_or_else(|| panic!("events.p declares no {event}"));
    let payload = decl.split_once('(').unwrap().1.rsplit_once(')').unwrap().0;
    payload.split(", ").map(|f| f.split(':').next().unwrap().trim().to_string()).filter(|f| f != "at").collect()
}

#[tokio::test]
async fn the_feed_logs_what_its_model_observes_for_a_replay() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    // A tokio test runs on one thread, so this catches the feed, the server, and the PDP alike.
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::new(format!("{}=debug", awzy_telemetry::MODEL_EVENTS)))
        .with_writer(captured.clone())
        .finish();
    let _logging = tracing::subscriber::set_default(subscriber);

    let (endpoint, mut client) = serve(store).await;
    let state = pdp(&endpoint).await;
    client
        .contain(ContainRequest {
            principal: Some(awzy_model::EntityRef::new("Human", "mallory").into()),
            reason: "bulk download of design docs".into(),
            created_by_actor: Some(awzy_model::Actor::awzy("awzy-reasoner").into()),
            ..Default::default()
        })
        .await
        .unwrap();
    until(|| async { check(&state, "mallory", "design-doc").await.verdict == Verdict::Deny }).await;

    let text = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    let lines: Vec<serde_json::Map<String, serde_json::Value>> = text
        .lines()
        .map(|l| {
            let line: serde_json::Map<String, serde_json::Value> = serde_json::from_str(l).unwrap();
            line.get("fields").and_then(|f| f.as_object()).cloned().unwrap()
        })
        .collect();
    let events = include_str!("../feed/events.p");
    for line in &lines {
        let event = line.get("model_event").and_then(|e| e.as_str()).unwrap();
        for field in declared_fields(events, event) {
            assert!(line.contains_key(&field), "{event} is logged without {field}: {line:?}");
        }
    }

    let field = |line: &serde_json::Map<String, serde_json::Value>, name: &str| line.get(name).cloned();
    let of = |event: &str| lines.iter().filter(move |l| field(l, "model_event") == Some(json!(event)));
    let changed = of("eStoreChanged")
        .find(|l| field(l, "contained").and_then(|c| c.as_str().map(|c| c.contains("Human::mallory"))) == Some(true))
        .expect("the store logs the version that contains mallory");
    let version = field(changed, "version").unwrap();
    assert!(of("eApplied").any(|l| field(l, "version") == Some(version.clone())), "the PDP logs applying it");
    assert!(
        of("eDecision").any(|l| field(l, "principal") == Some(json!("Human::mallory"))
            && field(l, "answer") == Some(json!(1))
            && field(l, "basis") == Some(version.clone())),
        "the PDP logs deciding from it that mallory is contained"
    );
}

use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use uuid::Uuid;

use super::*;

const WHY: &str = "Reviewing the Q3 board deck for the audit committee";

/// A store in a fresh Postgres schema, so tests never see each other's rows.
pub(crate) async fn test_store(lifetimes: Lifetimes) -> Option<ChallengeStore> {
    let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
        eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
        return None;
    };
    let schema = format!("test_{}", Uuid::now_v7().simple());
    let admin = PgPool::connect(&url).await.unwrap();
    sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&admin).await.unwrap();
    let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new().max_connections(2).connect_with(options).await.unwrap();
    let store = ChallengeStore::new(pool, lifetimes);
    store.migrate().await.unwrap();
    Some(store)
}

pub(crate) fn request(principal: EntityRef, on_behalf_of: Option<EntityRef>) -> RequestKey {
    RequestKey { principal, on_behalf_of, action: "read".into(), resource: EntityRef::new("Resource", "board-deck") }
}

fn alice() -> EntityRef {
    EntityRef::new("Human", "alice")
}

fn of(kinds: &[ChallengeKind]) -> Challenges {
    kinds.iter().copied().collect()
}

#[tokio::test]
async fn retries_share_one_pending_challenge() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let req = request(alice(), None);
    let required = of(&[ChallengeKind::Justification]);
    let first = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
    let again = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
    assert!(first.challenge_id.is_some());
    assert_eq!(first.challenge_id, again.challenge_id);
    assert!(first.passed.is_empty() && !first.rejected);

    let c = store.get(first.challenge_id.unwrap()).await.unwrap();
    assert_eq!((c.status, c.kinds, c.answerer_id.as_deref()), (ChallengeStatus::Pending, required, Some("alice")));
}

#[tokio::test]
async fn monitor_lookups_open_nothing() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let req = request(alice(), None);
    let r = store.resolve(DecisionId::new(), &req, of(&[ChallengeKind::Approval]), None, false).await.unwrap();
    assert_eq!(
        r,
        Resolution {
            passed: Challenges::default(),
            passed_policies: vec![],
            challenge_id: None,
            rejected: false,
            opened: false
        }
    );
    assert!(store.list(None, None, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_justification_grants_exactly_the_challenged_request() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let req = request(alice(), None);
    let required = of(&[ChallengeKind::Justification]);
    let id = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap().challenge_id;
    let id = id.unwrap();

    assert!(matches!(store.justify(id, "alice", "because").await, Err(StoreError::Invalid(_))));
    assert!(matches!(store.justify(id, "mallory", WHY).await, Err(StoreError::NotAllowed(_))));
    let c = store.justify(id, "alice", WHY).await.unwrap();
    assert_eq!((c.status, c.justification.as_deref()), (ChallengeStatus::Passed, Some(WHY)));
    assert!(c.grant_expires_at.unwrap() > Utc::now());
    assert!(matches!(store.justify(id, "alice", WHY).await, Err(StoreError::NotPending { .. })));

    let retry = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
    assert_eq!((retry.passed, retry.challenge_id), (required, None), "passed on retry, nothing new opened");

    let other = RequestKey { resource: EntityRef::new("Resource", "payroll"), ..req.clone() };
    let r = store.resolve(DecisionId::new(), &other, required, Some("alice"), true).await.unwrap();
    assert!(r.passed.is_empty() && r.challenge_id.is_some(), "another resource needs its own challenge");
    let write = RequestKey { action: "write".into(), ..req };
    let r = store.resolve(DecisionId::new(), &write, required, Some("alice"), true).await.unwrap();
    assert!(r.passed.is_empty(), "another action needs its own challenge");
}

#[tokio::test]
async fn every_kind_must_pass_before_the_grant() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let req = request(alice(), None);
    let required = of(&[ChallengeKind::Justification, ChallengeKind::Approval]);
    let id = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
    let id = id.challenge_id.unwrap();

    let c = store.justify(id, "alice", WHY).await.unwrap();
    assert_eq!((c.status, c.passed), (ChallengeStatus::Pending, of(&[ChallengeKind::Justification])));
    let r = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
    assert!(r.passed.is_empty(), "half a challenge grants nothing");
    assert_eq!(r.challenge_id, Some(id));

    let c = store.decide(id, "carol", true, "ok for the audit").await.unwrap();
    assert_eq!((c.status, c.decided_by.as_deref()), (ChallengeStatus::Passed, Some("carol")));
    let r = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
    assert_eq!((r.passed, r.challenge_id), (required, None));
}

#[tokio::test]
async fn a_grant_answers_only_the_policies_that_asked_for_it() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let req = request(alice(), None);
    let asked = |id: &str| ChallengedPolicy { id: id.into(), kind: ChallengeKind::Justification };
    let outside = [asked("outside-work")];
    let first = store.resolve_policies(DecisionId::new(), &req, &outside, Some("alice"), true).await.unwrap();
    let id = first.challenge_id.unwrap();
    store.justify(id, "alice", WHY).await.unwrap();

    let retry = store.resolve_policies(DecisionId::new(), &req, &outside, Some("alice"), true).await.unwrap();
    assert_eq!((retry.passed_policies, retry.challenge_id), (vec!["outside-work".to_string()], None));

    // Risk rose since: another policy asks for the same kind, and the earlier answer isn't one.
    let both = [asked("outside-work"), asked("risk-went-up")];
    let r = store.resolve_policies(DecisionId::new(), &req, &both, Some("alice"), true).await.unwrap();
    assert_eq!(r.passed_policies, ["outside-work"]);
    assert!(r.passed.is_empty(), "the kind isn't passed while one policy asking it is unanswered");
    let c = store.get(r.challenge_id.expect("a new challenge")).await.unwrap();
    assert_ne!(c.id, id);
}

#[tokio::test]
async fn a_policy_that_joins_a_pending_challenge_needs_its_kind_answered_again() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let req = request(alice(), None);
    let approval = ChallengedPolicy { id: "board-material".into(), kind: ChallengeKind::Approval };
    let justify = |id: &str| ChallengedPolicy { id: id.into(), kind: ChallengeKind::Justification };
    let first = [approval.clone(), justify("outside-work")];
    let id = store.resolve_policies(DecisionId::new(), &req, &first, Some("alice"), true).await.unwrap();
    let id = id.challenge_id.unwrap();
    store.justify(id, "alice", WHY).await.unwrap();

    // Before the approval, a policy asking for a justification of its own starts to apply.
    let more = [approval, justify("outside-work"), justify("risk-went-up")];
    let r = store.resolve_policies(DecisionId::new(), &req, &more, Some("alice"), true).await.unwrap();
    assert_eq!(r.challenge_id, Some(id), "the retry joins the pending challenge");
    let c = store.get(id).await.unwrap();
    assert!(!c.passed.contains(ChallengeKind::Justification), "the earlier justification wasn't for the new policy");

    store.decide(id, "carol", true, "").await.unwrap();
    assert_eq!(store.get(id).await.unwrap().status, ChallengeStatus::Pending, "a justification is still owed");
    store.justify(id, "alice", WHY).await.unwrap();
    let r = store.resolve_policies(DecisionId::new(), &req, &more, Some("alice"), true).await.unwrap();
    assert_eq!(r.passed_policies.len(), 3, "now the grant answers all three: {:?}", r.passed_policies);
}

#[tokio::test]
async fn no_one_approves_their_own_request() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let agent = EntityRef::new("ServiceAccount", "sales-genie");
    let req = request(agent, Some(EntityRef::new("Human", "bob")));
    let required = of(&[ChallengeKind::Approval]);
    let id = store.resolve(DecisionId::new(), &req, required, Some("bob"), true).await.unwrap();
    let id = id.challenge_id.unwrap();
    let err = store.decide(id, "bob", true, "").await;
    assert!(matches!(err, Err(StoreError::NotAllowed(_))), "the person the agent acts for");
    let err = store.justify(id, "bob", WHY).await;
    assert!(matches!(err, Err(StoreError::KindNotRequired { .. })), "approval can't be swapped for a justification");

    let human = request(alice(), None);
    let id = store.resolve(DecisionId::new(), &human, required, Some("alice"), true).await.unwrap();
    let err = store.decide(id.challenge_id.unwrap(), "alice", true, "").await;
    assert!(matches!(err, Err(StoreError::NotAllowed(_))), "the person asking");
}

#[tokio::test]
async fn an_agent_approves_but_never_its_own_request() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let req = request(EntityRef::new("ServiceAccount", "sales-genie"), Some(EntityRef::new("Human", "bob")));
    let required = of(&[ChallengeKind::Approval]);
    let id = store.resolve(DecisionId::new(), &req, required, Some("bob"), true).await.unwrap();
    let id = id.challenge_id.unwrap();
    let err = store.decide(id, "ServiceAccount::sales-genie", true, "").await;
    assert!(matches!(err, Err(StoreError::NotAllowed(_))), "the agent asking");

    let c = store.decide(id, "ServiceAccount::overseer", true, "routine").await.unwrap();
    assert_eq!((c.status, c.decided_by.as_deref()), (ChallengeStatus::Passed, Some("ServiceAccount::overseer")));
}

#[tokio::test]
async fn a_rejection_blocks_the_request_until_it_expires() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let req = request(alice(), None);
    let required = of(&[ChallengeKind::Approval]);
    let id = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
    let c = store.decide(id.challenge_id.unwrap(), "carol", false, "not for this quarter").await.unwrap();
    assert_eq!(c.status, ChallengeStatus::Rejected);

    let r = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
    assert!(r.rejected && r.challenge_id.is_none(), "no new challenge while the rejection stands");
}

#[tokio::test]
async fn expired_challenges_and_grants_count_for_nothing() {
    let lifetimes = Lifetimes { pending: Duration::ZERO, grant: Duration::ZERO };
    let Some(store) = test_store(lifetimes).await else { return };
    let req = request(alice(), None);
    let required = of(&[ChallengeKind::Justification]);
    let first = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
    let first = first.challenge_id.unwrap();
    assert_eq!(store.get(first).await.unwrap().status, ChallengeStatus::Expired);
    assert!(matches!(store.justify(first, "alice", WHY).await, Err(StoreError::NotPending { .. })));

    let second = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
    assert_ne!(second.challenge_id, Some(first), "an expired challenge is replaced");
    assert_eq!(store.list(Some("alice"), Some(ChallengeStatus::Expired), 10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn containments_are_idempotent_lifted_once_and_versioned() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    assert_eq!(store.active_containments().await.unwrap(), ActiveContainments::default());

    let agent = EntityRef::new("ServiceAccount", "sales-genie");
    let (c, created) = store.contain(&agent, "bulk export of pipeline", "carol", false).await.unwrap();
    assert!(created);
    let (again, created) = store.contain(&agent, "again", "dave", false).await.unwrap();
    assert_eq!((again.id, created), (c.id, false));
    let active = store.active_containments().await.unwrap();
    assert_eq!((active.version, active.active.len()), (1, 1));

    assert!(matches!(store.lift(c.id, "carol", " ").await, Err(StoreError::Invalid(_))));
    let lifted = store.lift(c.id, "carol", "reviewed: expected export").await.unwrap();
    assert_eq!(lifted.lifted_by.as_deref(), Some("carol"));
    assert!(matches!(store.lift(c.id, "carol", "again").await, Err(StoreError::AlreadyLifted(_))));
    let active = store.active_containments().await.unwrap();
    assert_eq!((active.version, active.active.len()), (2, 0));

    let (next, created) = store.contain(&agent, "again, later", "carol", false).await.unwrap();
    assert!(created && next.id != c.id, "a lifted containment doesn't block a new one");
    assert_eq!(store.containments(false, Some(&agent), 10, false).await.unwrap().len(), 2);
    assert_eq!(store.containments(true, None, 10, false).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_monitored_containment_stays_out_of_the_feed_and_beside_a_real_one() {
    let Some(store) = test_store(Lifetimes::default()).await else { return };
    let nora = EntityRef::new("Human", "nora");
    let (would, created) =
        store.contain(&nora, "rejected challenge and an export", "AwzyService::awzy-reasoner", true).await.unwrap();
    assert!(created && would.monitor);
    let (again, created) = store.contain(&nora, "same evidence", "AwzyService::awzy-reasoner", true).await.unwrap();
    assert_eq!((again.id, created), (would.id, false));
    let active = store.active_containments().await.unwrap();
    assert_eq!((active.version, active.active.len()), (0, 0), "PDPs never hear of it");

    // The security team agrees and contains her for real; the monitored record stays.
    let (real, created) = store.contain(&nora, "confirmed on review", "Human::carol", false).await.unwrap();
    assert!(created && !real.monitor && real.id != would.id);
    assert_eq!(store.active_containments().await.unwrap().active, vec![real]);
    assert_eq!(store.containments(true, Some(&nora), 10, true).await.unwrap().len(), 2);
    assert_eq!(store.containments(true, Some(&nora), 10, false).await.unwrap().len(), 1);
}

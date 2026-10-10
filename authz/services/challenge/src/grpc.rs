//! `awzy.challenge.v1.ChallengeService` over gRPC.

use std::{pin::Pin, sync::Arc};

use awzy_authz_proto::{
    challenge::v1::{
        challenge_service_server::{ChallengeService, ChallengeServiceServer},
        Challenge as WireChallenge, ChallengeStatus as WireStatus, ContainRequest, ContainResponse,
        Containment as WireContainment, DecideRequest, DecideResponse, GetChallengeRequest, GetChallengeResponse,
        JustifyRequest, JustifyResponse, LiftRequest, LiftResponse, ListChallengesRequest, ListChallengesResponse,
        ListContainmentsRequest, ListContainmentsResponse, ResolveRequest, ResolveResponse, WatchContainmentsRequest,
        WatchContainmentsResponse,
    },
    challenges_from_wire, challenges_to_wire,
    prost_types::Timestamp,
};
use awzy_model::{ContainmentId, DecisionId, EntityRef};
use axum::Router;
use chrono::{DateTime, Utc};
// tonic's own re-export, so the stream wrapper needs no dependency of its own.
use tonic::codegen::tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{Request, Response, Status};

use crate::{
    approvals::{ApprovalError, Approvals},
    feed::{Feed, HEARTBEAT},
    store::{Challenge, ChallengeStatus, ChallengeStore, Containment, RequestKey, StoreError},
};

pub fn router(store: Arc<ChallengeStore>, feed: Arc<Feed>, approvals: Arc<Approvals>) -> Router {
    tonic::service::Routes::new(ChallengeServiceServer::new(Service { store, feed, approvals })).into_axum_router()
}

struct Service {
    store: Arc<ChallengeStore>,
    feed: Arc<Feed>,
    approvals: Arc<Approvals>,
}

type FeedStream = Pin<Box<dyn Stream<Item = Result<WatchContainmentsResponse, Status>> + Send>>;

#[tonic::async_trait]
impl ChallengeService for Service {
    async fn resolve(&self, req: Request<ResolveRequest>) -> Result<Response<ResolveResponse>, Status> {
        let req = req.into_inner();
        let decision_id = qid::<DecisionId>(&req.decision_id)?;
        let required = challenges_from_wire(&req.kinds);
        if required.is_empty() {
            return Err(Status::invalid_argument("kinds is required"));
        }
        let request = RequestKey {
            subject: required_ref(req.subject, "subject")?,
            on_behalf_of: req.on_behalf_of.map(Into::into),
            action: req.action,
            resource: required_ref(req.resource, "resource")?,
        };
        let answerer = Some(req.answerer_id.as_str()).filter(|a| !a.is_empty());
        let r = self.store.resolve(decision_id, &request, required, answerer, req.open).await.map_err(status)?;
        if let Some(id) = r.challenge_id {
            tracing::info!(challenge_id = %id, %decision_id, kinds = %required.without(r.passed).joined(), "challenge open");
        }
        Ok(Response::new(ResolveResponse {
            passed: challenges_to_wire(r.passed),
            challenge_id: r.challenge_id.map(|id| id.to_string()).unwrap_or_default(),
            rejected: r.rejected,
        }))
    }

    async fn get_challenge(&self, req: Request<GetChallengeRequest>) -> Result<Response<GetChallengeResponse>, Status> {
        let c = self.store.get(qid(&req.into_inner().id)?).await.map_err(status)?;
        Ok(Response::new(GetChallengeResponse { challenge: Some(challenge_to_wire(c)) }))
    }

    async fn list_challenges(
        &self,
        req: Request<ListChallengesRequest>,
    ) -> Result<Response<ListChallengesResponse>, Status> {
        let req = req.into_inner();
        let status_filter = match req.status() {
            WireStatus::Unspecified => None,
            WireStatus::Pending => Some(ChallengeStatus::Pending),
            WireStatus::Passed => Some(ChallengeStatus::Passed),
            WireStatus::Rejected => Some(ChallengeStatus::Rejected),
            WireStatus::Expired => Some(ChallengeStatus::Expired),
        };
        let answerer = Some(req.answerer_id.as_str()).filter(|a| !a.is_empty());
        let challenges = self.store.list(answerer, status_filter, limit(req.limit)).await.map_err(status)?;
        Ok(Response::new(ListChallengesResponse {
            challenges: challenges.into_iter().map(challenge_to_wire).collect(),
        }))
    }

    async fn justify(&self, req: Request<JustifyRequest>) -> Result<Response<JustifyResponse>, Status> {
        let req = req.into_inner();
        if req.answered_by.is_empty() {
            return Err(Status::invalid_argument("answered_by is required"));
        }
        let c = self.store.justify(qid(&req.id)?, &req.answered_by, &req.justification).await.map_err(status)?;
        tracing::info!(challenge_id = %c.id, answered_by = %req.answered_by, status = c.status.as_str(), "challenge justified");
        Ok(Response::new(JustifyResponse { challenge: Some(challenge_to_wire(c)) }))
    }

    async fn decide(&self, req: Request<DecideRequest>) -> Result<Response<DecideResponse>, Status> {
        let req = req.into_inner();
        if req.decided_by.is_empty() {
            return Err(Status::invalid_argument("decided_by is required"));
        }
        let id = qid(&req.id)?;
        // Checked before the store's own checks so a refusal names the policy decision; the store
        // then re-checks the challenge is still pending as it records the answer.
        let c = self.store.get(id).await.map_err(status)?;
        self.approvals.check(&req.decided_by, &c.request.resource).await.map_err(|e| match e {
            ApprovalError::Refused(_) => Status::permission_denied(e.to_string()),
            ApprovalError::Unavailable(_) => Status::unavailable(e.to_string()),
        })?;
        let c = self.store.decide(id, &req.decided_by, req.approve, &req.note).await.map_err(status)?;
        tracing::info!(challenge_id = %c.id, decided_by = %req.decided_by, approve = req.approve, "challenge decided");
        Ok(Response::new(DecideResponse { challenge: Some(challenge_to_wire(c)) }))
    }

    async fn contain(&self, req: Request<ContainRequest>) -> Result<Response<ContainResponse>, Status> {
        let req = req.into_inner();
        let subject = required_ref(req.subject, "subject")?;
        let (c, created) = self.store.contain(&subject, &req.reason, &req.created_by).await.map_err(status)?;
        if created {
            tracing::warn!(containment_id = %c.id, subject = ?c.subject, created_by = %c.created_by, reason = %c.reason, "accessor contained");
            self.refresh_feed().await;
        }
        Ok(Response::new(ContainResponse { containment: Some(containment_to_wire(c)) }))
    }

    async fn lift(&self, req: Request<LiftRequest>) -> Result<Response<LiftResponse>, Status> {
        let req = req.into_inner();
        let c = self.store.lift(qid::<ContainmentId>(&req.id)?, &req.lifted_by, &req.reason).await.map_err(status)?;
        tracing::warn!(containment_id = %c.id, subject = ?c.subject, lifted_by = %req.lifted_by, "containment lifted");
        self.refresh_feed().await;
        Ok(Response::new(LiftResponse { containment: Some(containment_to_wire(c)) }))
    }

    async fn list_containments(
        &self,
        req: Request<ListContainmentsRequest>,
    ) -> Result<Response<ListContainmentsResponse>, Status> {
        let req = req.into_inner();
        let subject: Option<EntityRef> = req.subject.map(Into::into);
        let list =
            self.store.containments(req.active_only, subject.as_ref(), limit(req.limit)).await.map_err(status)?;
        Ok(Response::new(ListContainmentsResponse {
            containments: list.into_iter().map(containment_to_wire).collect(),
        }))
    }

    type WatchContainmentsStream = FeedStream;

    async fn watch_containments(
        &self,
        _req: Request<WatchContainmentsRequest>,
    ) -> Result<Response<Self::WatchContainmentsStream>, Status> {
        let mut rx = self.feed.subscribe();
        let (tx, out) = tokio::sync::mpsc::channel(4);
        tokio::spawn(async move {
            let mut sent = None;
            loop {
                let current = rx.borrow_and_update().clone();
                // Nothing until the first load: an empty list would read as "no one is contained".
                if let Some(list) = current {
                    let changed = sent != Some(list.version);
                    sent = Some(list.version);
                    let active =
                        if changed { list.active.iter().cloned().map(containment_to_wire).collect() } else { vec![] };
                    if tx.send(Ok(WatchContainmentsResponse { version: list.version, changed, active })).await.is_err()
                    {
                        return; // the PDP went away
                    }
                }
                tokio::select! {
                    changed = rx.changed() => if changed.is_err() { return },
                    _ = tokio::time::sleep(HEARTBEAT) => {}
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(out))))
    }
}

impl Service {
    /// Publishes a change made here at once, without waiting for the notification round trip.
    async fn refresh_feed(&self) {
        if let Err(error) = self.feed.refresh(&self.store).await {
            tracing::warn!(%error, "couldn't refresh the containment feed; the next reload will");
        }
    }
}

fn qid<T: std::str::FromStr<Err = awzy_model::QidError>>(s: &str) -> Result<T, Status> {
    s.parse().map_err(|e: awzy_model::QidError| Status::invalid_argument(e.to_string()))
}

fn required_ref(r: Option<awzy_authz_proto::authz::v1::EntityRef>, what: &str) -> Result<EntityRef, Status> {
    match r {
        Some(r) if !r.r#type.is_empty() && !r.id.is_empty() => Ok(r.into()),
        _ => Err(Status::invalid_argument(format!("{what} is required"))),
    }
}

fn limit(n: i32) -> i64 {
    if n <= 0 {
        50
    } else {
        n.min(500).into()
    }
}

fn status(e: StoreError) -> Status {
    match e {
        StoreError::ChallengeNotFound(_) | StoreError::ContainmentNotFound(_) => Status::not_found(e.to_string()),
        StoreError::NotPending { .. }
        | StoreError::KindNotRequired { .. }
        | StoreError::AlreadyPassed { .. }
        | StoreError::AlreadyLifted(_) => Status::failed_precondition(e.to_string()),
        StoreError::NotAllowed(_) => Status::permission_denied(e.to_string()),
        StoreError::Invalid(_) => Status::invalid_argument(e.to_string()),
        StoreError::Db(error) => {
            tracing::error!(%error, "challenge store failed");
            Status::internal("challenge store unavailable")
        }
    }
}

fn ts(t: DateTime<Utc>) -> Timestamp {
    Timestamp { seconds: t.timestamp(), nanos: t.timestamp_subsec_nanos() as i32 }
}

fn challenge_to_wire(c: Challenge) -> WireChallenge {
    WireChallenge {
        id: c.id.to_string(),
        decision_id: c.decision_id.to_string(),
        subject: Some(c.request.subject.into()),
        on_behalf_of: c.request.on_behalf_of.map(Into::into),
        action: c.request.action,
        resource: Some(c.request.resource.into()),
        kinds: challenges_to_wire(c.kinds),
        passed: challenges_to_wire(c.passed),
        status: match c.status {
            ChallengeStatus::Pending => WireStatus::Pending,
            ChallengeStatus::Passed => WireStatus::Passed,
            ChallengeStatus::Rejected => WireStatus::Rejected,
            ChallengeStatus::Expired => WireStatus::Expired,
        }
        .into(),
        answerer_id: c.answerer_id.unwrap_or_default(),
        justification: c.justification.unwrap_or_default(),
        decided_by: c.decided_by.unwrap_or_default(),
        decision_note: c.decision_note.unwrap_or_default(),
        created_at: Some(ts(c.created_at)),
        expires_at: Some(ts(c.expires_at)),
        passed_at: c.passed_at.map(ts),
        grant_expires_at: c.grant_expires_at.map(ts),
    }
}

fn containment_to_wire(c: Containment) -> WireContainment {
    WireContainment {
        id: c.id.to_string(),
        subject: Some(c.subject.into()),
        reason: c.reason,
        created_by: c.created_by,
        created_at: Some(ts(c.created_at)),
        lifted_at: c.lifted_at.map(ts),
        lifted_by: c.lifted_by.unwrap_or_default(),
        lift_reason: c.lift_reason.unwrap_or_default(),
    }
}

/// End to end against Postgres: an awzy-pdp in enforce mode opens challenges here, honors them
/// once passed, and follows the containment feed.
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use awzy_authz_proto::challenge::v1::challenge_service_client::ChallengeServiceClient;
    use awzy_model::{AccessRequest, EnforcementMode, Verdict};
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
        let approvals = Arc::new(Approvals::Only(vec!["carol".into()]));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let app = router(store, feed, approvals);
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

    fn read(who: &str, what: &str) -> (AccessRequest, serde_json::Value) {
        let sensitivity = match what {
            "board-deck" => "crown_jewels",
            "payroll" => "confidential",
            _ => "internal",
        };
        let req = AccessRequest {
            subject: awzy_model::EntityRef::new("Human", who),
            action: "read".into(),
            resource: awzy_model::EntityRef::new("Resource", what),
            context: Default::default(),
            on_behalf_of: None,
            human_present: None,
            always_enforced: false,
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
                id: challenge_id.to_string(),
                answered_by: "alice".into(),
                justification: "Preparing the audit committee's review of Q3".into(),
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
    async fn approval_comes_from_someone_policy_allows() {
        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let (endpoint, mut client) = serve(store).await;
        let state = pdp(&endpoint).await;

        let id = check(&state, "bob", "payroll").await.challenge_id.unwrap().to_string();
        let decide = |by: &str, approve: bool| DecideRequest {
            id: id.clone(),
            decided_by: by.into(),
            approve,
            note: String::new(),
        };
        let refused = client.decide(decide("dave", true)).await.unwrap_err();
        assert_eq!(refused.code(), Code::PermissionDenied);
        client.decide(decide("carol", true)).await.unwrap();
        assert_eq!(check(&state, "bob", "payroll").await.verdict, Verdict::Allow);

        let id = check(&state, "erin", "payroll").await.challenge_id.unwrap().to_string();
        client
            .decide(DecideRequest { id, decided_by: "carol".into(), approve: false, note: "no".into() })
            .await
            .unwrap();
        let rejected = check(&state, "erin", "payroll").await;
        assert_eq!(rejected.verdict, Verdict::Deny);
        assert!(rejected.policies.contains(&awzy_pdp::challenges::CHALLENGE_REJECTED.to_string()));
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
                subject: Some(awzy_model::EntityRef::new("Human", "mallory").into()),
                reason: "bulk download of design docs".into(),
                created_by: "carol".into(),
            })
            .await
            .unwrap()
            .into_inner()
            .containment
            .unwrap();
        until(|| async { check(&state, "mallory", "design-doc").await.verdict == Verdict::Deny }).await;
        assert!(started.elapsed() < Duration::from_secs(2), "took {:?}", started.elapsed());
        assert_eq!(check(&state, "mallory", "design-doc").await.policies, std::slice::from_ref(&containment.id));
        assert_eq!(check(&state, "alice", "design-doc").await.verdict, Verdict::Allow, "only mallory");

        client
            .lift(LiftRequest { id: containment.id, lifted_by: "carol".into(), reason: "reviewed".into() })
            .await
            .unwrap();
        until(|| async { check(&state, "mallory", "design-doc").await.verdict == Verdict::Allow }).await;
    }
}

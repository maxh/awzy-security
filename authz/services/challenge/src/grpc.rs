//! `awzy.challenge.v1.ChallengeService` over gRPC.

use awzy_service_auth::{require_by_path, Allowed, PathGuard, Rule};
use std::{pin::Pin, sync::Arc};

use awzy_authz_proto::{
    challenge::v1::{
        challenge_service_server::{ChallengeService, ChallengeServiceServer},
        Assertion as WireAssertion, BeginKeyChangeRequest, BeginKeyChangeResponse, BeginKeyRegistrationRequest,
        BeginKeyRegistrationResponse, BeginVerificationRequest, BeginVerificationResponse, Challenge as WireChallenge,
        ChallengeStatus as WireStatus, ContainRequest, ContainResponse, Containment as WireContainment, DecideRequest,
        DecideResponse, FinishKeyRegistrationRequest, FinishKeyRegistrationResponse, FinishVerificationRequest,
        FinishVerificationResponse, GetChallengeRequest, GetChallengeResponse, JustifyRequest, JustifyResponse,
        LiftRequest, LiftResponse, ListChallengesRequest, ListChallengesResponse, ListContainmentsRequest,
        ListContainmentsResponse, ListSecurityKeysRequest, ListSecurityKeysResponse, RemoveSecurityKeyRequest,
        RemoveSecurityKeyResponse, ReportBreakGlassRequest, ReportBreakGlassResponse, RequestLiftRequest,
        RequestLiftResponse, ResetSecurityKeyRequest, ResetSecurityKeyResponse, ResolveRequest, ResolveResponse,
        SecurityKey as WireKey, WatchChallengesRequest, WatchChallengesResponse, WatchContainmentsRequest,
        WatchContainmentsResponse,
    },
    challenge::v1::{
        DecideSupportSessionRequest, DecideSupportSessionResponse, EndSupportSessionRequest, EndSupportSessionResponse,
        GetSupportSessionRequest, GetSupportSessionResponse, ListSupportSessionsRequest, ListSupportSessionsResponse,
        RecordSupportActionRequest, RecordSupportActionResponse, RecordSupportRequestsRequest,
        RecordSupportRequestsResponse,
    },
    challenges_from_wire, challenges_to_wire,
    prost_types::Timestamp,
};
use awzy_model::{ContainmentId, DecisionId, EntityRef, SupportSessionId};
use axum::Router;
use chrono::{DateTime, Utc};
// tonic's own re-export, so the stream wrapper needs no dependency of its own.
use tonic::codegen::tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{Request, Response, Status};

use crate::{
    approvals::{ApprovalError, Approvals},
    feed::{Feed, HEARTBEAT},
    keys::{Assertion, KeyError, SecurityKey, SecurityKeys},
    store::{Challenge, ChallengeStatus, ChallengeStore, ChallengedPolicy, Containment, RequestKey, StoreError},
    support::{self, SupportError, SupportStore},
};

/// Who may call which methods (ADR 0055). Each group's callers come from the chart, as
/// `namespace/service-account`; a method no group names is refused.
pub struct Access {
    verifier: Arc<awzy_service_auth::Verifier>,
    /// Resolving challenges and following containments: the PDPs.
    pdps: Allowed,
    /// Reading and answering challenges for a person (`answered_by`, `decided_by` are taken as
    /// given): only the client portal, which authenticates that person.
    answerers: Allowed,
    /// Containing and lifting: the client portal, and later awzy-reasoner.
    containers: Allowed,
    /// Following containments, so an outage never replays an allow for a contained principal:
    /// the MCP gateways (ADR 0030).
    gateways: Allowed,
    /// Reporting break-glass rollbacks: awzy-policy.
    policies: Allowed,
    /// Bringing in support session requests from the corp portal (ADR 0095): awzy-reporter.
    reporters: Allowed,
}

/// Callers either group allows.
fn either(a: &Allowed, b: &Allowed) -> Allowed {
    match (a, b) {
        (Allowed::Only(a), Allowed::Only(b)) => Allowed::Only(a.iter().chain(b).cloned().collect()),
        _ => Allowed::AnyServiceAccount,
    }
}

impl Access {
    /// From `AWZY_CHALLENGE_ANSWERERS`, `AWZY_CHALLENGE_CONTAINERS`, `AWZY_CHALLENGE_GATEWAYS`,
    /// `AWZY_CHALLENGE_PDPS`, `AWZY_CHALLENGE_POLICY`, and `AWZY_CHALLENGE_REPORTERS`.
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            verifier: Arc::new(awzy_service_auth::Verifier::from_env("awzy-challenge")?),
            pdps: Allowed::from_env("AWZY_CHALLENGE_PDPS"),
            answerers: Allowed::from_env("AWZY_CHALLENGE_ANSWERERS"),
            containers: Allowed::from_env("AWZY_CHALLENGE_CONTAINERS"),
            gateways: Allowed::from_env("AWZY_CHALLENGE_GATEWAYS"),
            policies: Allowed::from_env("AWZY_CHALLENGE_POLICY"),
            reporters: Allowed::from_env("AWZY_CHALLENGE_REPORTERS"),
        })
    }

    /// Everyone may call everything: for tests and local runs.
    pub fn off() -> Self {
        let nobody = || Allowed::only(&[]);
        Self {
            verifier: Arc::new(awzy_service_auth::Verifier::off()),
            pdps: nobody(),
            answerers: nobody(),
            containers: nobody(),
            gateways: nobody(),
            policies: nobody(),
            reporters: nobody(),
        }
    }

    fn guard(&self) -> PathGuard {
        let method = |m: &str| format!("/awzy.challenge.v1.ChallengeService/{m}");
        let group = |methods: &[&str], allowed: &Allowed| -> Vec<(String, Rule)> {
            methods.iter().map(|m| (method(m), Rule::Allow(allowed.clone()))).collect()
        };
        let mut rules = group(&["Resolve"], &self.pdps);
        // Following containments: the PDPs, the gateways for their outage fallback, and the client
        // portal for its live Challenges page.
        rules.extend(group(&["WatchContainments"], &either(&either(&self.pdps, &self.gateways), &self.containers)));
        rules.extend(group(&["WatchChallenges"], &either(&self.answerers, &self.containers)));
        // Listing containments: a PDP whose feed is stale, and the portal showing them to admins.
        rules.extend(group(&["ListContainments"], &either(&self.pdps, &self.containers)));
        rules.extend(group(&["GetChallenge", "Justify", "Decide"], &self.answerers));
        // Listing challenges: the portal, and awzy-reasoner reading rejections (ADR 0066).
        rules.extend(group(&["ListChallenges"], &either(&self.answerers, &self.containers)));
        // Security keys (ADR 0059): only the portal, which authenticates the person they belong to.
        rules.extend(group(
            &[
                "ListSecurityKeys",
                "BeginKeyChange",
                "BeginKeyRegistration",
                "FinishKeyRegistration",
                "RemoveSecurityKey",
                "ResetSecurityKey",
                "BeginVerification",
                "FinishVerification",
            ],
            &self.answerers,
        ));
        rules.extend(group(&["Contain", "Lift"], &self.containers));
        // Asking for a lift: the portal, for whoever signed in to it.
        rules.extend(group(&["RequestLift"], &self.answerers));
        rules.extend(group(&["ReportBreakGlass"], &self.policies));
        // Support sessions (ADR 0095): the reporter brings requests in; the portal does the rest,
        // for the admin or the staff member it authenticated.
        rules.extend(group(&["RecordSupportRequests"], &self.reporters));
        rules.extend(group(
            &[
                "ListSupportSessions",
                "GetSupportSession",
                "DecideSupportSession",
                "EndSupportSession",
                "RecordSupportAction",
            ],
            &self.answerers,
        ));
        PathGuard::new(self.verifier.clone(), rules)
    }
}

/// `keys`: None when no WebAuthn relying party is configured; the security-key RPCs then refuse.
/// `access`: who may call which methods.
pub fn router(
    store: Arc<ChallengeStore>,
    feed: Arc<Feed>,
    approvals: Arc<Approvals>,
    keys: Option<Arc<SecurityKeys>>,
    access: &Access,
) -> Router {
    let support = SupportStore::new(store.pool().clone());
    tonic::service::Routes::new(ChallengeServiceServer::new(Service { store, feed, approvals, keys, support }))
        .into_axum_router()
        .route_layer(axum::middleware::from_fn_with_state(access.guard(), require_by_path))
}

struct Service {
    store: Arc<ChallengeStore>,
    feed: Arc<Feed>,
    approvals: Arc<Approvals>,
    keys: Option<Arc<SecurityKeys>>,
    support: SupportStore,
}

type FeedStream = Pin<Box<dyn Stream<Item = Result<WatchContainmentsResponse, Status>> + Send>>;
type ChallengeStream = Pin<Box<dyn Stream<Item = Result<WatchChallengesResponse, Status>> + Send>>;

/// How often `WatchChallenges` says it's alive when no challenge changes.
const CHALLENGE_HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(15);

#[tonic::async_trait]
impl ChallengeService for Service {
    async fn record_support_requests(
        &self,
        req: Request<RecordSupportRequestsRequest>,
    ) -> Result<Response<RecordSupportRequestsResponse>, Status> {
        let requests: Vec<support::Request> =
            req.into_inner().requests.into_iter().filter_map(|r| support::request_from_wire(r).ok()).collect();
        let sessions = self.support.record_requests(&requests).await.map_err(support_status)?;
        Ok(Response::new(RecordSupportRequestsResponse {
            sessions: sessions.into_iter().map(support::session_to_wire).collect(),
        }))
    }

    async fn list_support_sessions(
        &self,
        req: Request<ListSupportSessionsRequest>,
    ) -> Result<Response<ListSupportSessionsResponse>, Status> {
        let sessions = self.support.list(limit(req.into_inner().limit)).await.map_err(support_status)?;
        Ok(Response::new(ListSupportSessionsResponse {
            sessions: sessions.into_iter().map(support::session_to_wire).collect(),
        }))
    }

    async fn get_support_session(
        &self,
        req: Request<GetSupportSessionRequest>,
    ) -> Result<Response<GetSupportSessionResponse>, Status> {
        let id = qid::<SupportSessionId>(&req.into_inner().qid)?;
        let session = self.support.get(id).await.map_err(support_status)?;
        Ok(Response::new(GetSupportSessionResponse { session: Some(support::session_to_wire(session)) }))
    }

    async fn decide_support_session(
        &self,
        req: Request<DecideSupportSessionRequest>,
    ) -> Result<Response<DecideSupportSessionResponse>, Status> {
        let req = req.into_inner();
        let id = qid::<SupportSessionId>(&req.qid)?;
        // Only a person decides, acting for themselves.
        human(req.decided_by_actor.clone(), "", "decided_by_actor")?;
        let by = actor(req.decided_by_actor, "", "decided_by_actor")?;
        let session =
            self.support.decide(id, req.approve, &req.areas, req.minutes, &by).await.map_err(support_status)?;
        Ok(Response::new(DecideSupportSessionResponse { session: Some(support::session_to_wire(session)) }))
    }

    async fn end_support_session(
        &self,
        req: Request<EndSupportSessionRequest>,
    ) -> Result<Response<EndSupportSessionResponse>, Status> {
        let req = req.into_inner();
        let id = qid::<SupportSessionId>(&req.qid)?;
        human(req.ended_by_actor.clone(), "", "ended_by_actor")?;
        let by = actor(req.ended_by_actor, "", "ended_by_actor")?;
        let session = self.support.end(id, &by).await.map_err(support_status)?;
        Ok(Response::new(EndSupportSessionResponse { session: Some(support::session_to_wire(session)) }))
    }

    async fn record_support_action(
        &self,
        req: Request<RecordSupportActionRequest>,
    ) -> Result<Response<RecordSupportActionResponse>, Status> {
        let req = req.into_inner();
        let id = qid::<SupportSessionId>(&req.qid)?;
        self.support.record_action(id, &req.operation).await.map_err(support_status)?;
        Ok(Response::new(RecordSupportActionResponse {}))
    }

    async fn resolve(&self, req: Request<ResolveRequest>) -> Result<Response<ResolveResponse>, Status> {
        let req = req.into_inner();
        let decision_id = qid::<DecisionId>(&req.decision_qid)?;
        let policies: Vec<ChallengedPolicy> = if req.policies.is_empty() {
            challenges_from_wire(&req.kinds).iter().map(ChallengedPolicy::for_kind).collect()
        } else {
            req.policies
                .iter()
                .map(|p| {
                    let kind = challenges_from_wire(&[p.kind]).iter().next();
                    match kind {
                        Some(kind) if !p.policy_id.is_empty() => Ok(ChallengedPolicy { id: p.policy_id.clone(), kind }),
                        _ => Err(Status::invalid_argument("each policy needs its ID and the kind it asks for")),
                    }
                })
                .collect::<Result<_, _>>()?
        };
        if policies.is_empty() {
            return Err(Status::invalid_argument("kinds or policies is required"));
        }
        let required: awzy_model::Challenges = policies.iter().map(|p| p.kind).collect();
        let request = RequestKey {
            principal: required_ref(req.principal, "principal")?,
            on_behalf_of: req.on_behalf_of.map(Into::into),
            action: req.action,
            resource: required_ref(req.resource, "resource")?,
        };
        let answerer = Some(req.answerer_id.as_str()).filter(|a| !a.is_empty());
        let r =
            self.store.resolve_policies(decision_id, &request, &policies, answerer, req.open).await.map_err(status)?;
        if let Some(id) = r.challenge_id {
            tracing::info!(challenge_id = %id, %decision_id, kinds = %required.without(r.passed).joined(), "challenge open");
            // Tell people only about a new challenge, not each retry (ADR 0073).
            let owner = Some(req.resource_owner_id.as_str()).filter(|o| !o.is_empty());
            if r.opened {
                if let Err(e) = self.store.notify_opened(id, owner).await {
                    tracing::warn!(challenge_id = %id, error = %e, "couldn't record who to tell about a challenge");
                }
            }
        }
        Ok(Response::new(ResolveResponse {
            passed: challenges_to_wire(r.passed),
            challenge_qid: r.challenge_id.map(|id| id.to_string()).unwrap_or_default(),
            rejected: r.rejected,
            passed_policies: r.passed_policies,
        }))
    }

    async fn get_challenge(&self, req: Request<GetChallengeRequest>) -> Result<Response<GetChallengeResponse>, Status> {
        let c = self.store.get(qid(&req.into_inner().challenge_qid)?).await.map_err(status)?;
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
        let answered_by = human(req.answered_by_actor, &req.answered_by, "answered_by_actor")?;
        let c = self.store.justify(qid(&req.challenge_qid)?, &answered_by, &req.justification).await.map_err(status)?;
        tracing::info!(challenge_id = %c.id, answered_by = %answered_by, status = c.status.as_str(), "challenge justified");
        Ok(Response::new(JustifyResponse { challenge: Some(challenge_to_wire(c)) }))
    }

    async fn decide(&self, req: Request<DecideRequest>) -> Result<Response<DecideResponse>, Status> {
        let req = req.into_inner();
        let approver = approver(req.decided_by_actor.clone(), &req.decided_by)?;
        // A person is stored by their bare ID, as before ADR 0060; anyone else in the typed form.
        let decided_by = if approver.subject.kind == awzy_model::entity_type::HUMAN {
            approver.subject.id.clone()
        } else {
            approver.to_string()
        };
        let id = qid(&req.challenge_qid)?;
        // Checked before the store's own checks so a refusal names the policy decision; the store
        // then re-checks the challenge is still pending as it records the answer.
        let c = self.store.get(id).await.map_err(status)?;
        let checked = self.approvals.check(
            &approver.subject,
            &c.request.resource,
            c.answerer_id.as_deref(),
            req.approver_entities.clone(),
        );
        checked.await.map_err(|e| match e {
            ApprovalError::Refused(_) => Status::permission_denied(e.to_string()),
            ApprovalError::Unavailable(_) => Status::unavailable(e.to_string()),
        })?;
        let c = self.store.decide(id, &decided_by, req.approve, &req.note).await.map_err(status)?;
        tracing::info!(challenge_id = %c.id, decided_by = %decided_by, approve = req.approve, "challenge decided");
        if let Err(e) = self.store.notify_decided(&c, req.approve).await {
            tracing::warn!(challenge_id = %c.id, error = %e, "couldn't record who to tell about a decision");
        }
        Ok(Response::new(DecideResponse { challenge: Some(challenge_to_wire(c)) }))
    }

    async fn report_break_glass(
        &self,
        req: Request<ReportBreakGlassRequest>,
    ) -> Result<Response<ReportBreakGlassResponse>, Status> {
        let req = req.into_inner();
        let details = serde_json::json!({
            "key_id": req.key_id,
            "reason": req.reason,
            "restored_version": req.restored_version,
            "policy_bundle_qid": req.policy_bundle_qid,
            "version": req.version,
        });
        self.store.notify_break_glass(&details).await.map_err(status)?;
        Ok(Response::new(ReportBreakGlassResponse {}))
    }

    async fn contain(&self, req: Request<ContainRequest>) -> Result<Response<ContainResponse>, Status> {
        let req = req.into_inner();
        let principal = required_ref(req.principal, "principal")?;
        let created_by = actor(req.created_by_actor, &req.created_by, "created_by_actor")?;
        let (c, created) =
            self.store.contain(&principal, &req.reason, &created_by, req.monitor).await.map_err(status)?;
        if created {
            if let Err(e) = self.store.notify_contained(&c).await {
                tracing::warn!(containment_id = %c.id, error = %e, "couldn't record who to tell about a containment");
            }
        }
        if created && c.monitor {
            tracing::info!(containment_id = %c.id, principal = ?c.principal, created_by = %c.created_by, reason = %c.reason, "principal would be contained (monitor)");
        } else if created {
            tracing::warn!(containment_id = %c.id, principal = ?c.principal, created_by = %c.created_by, reason = %c.reason, "principal contained");
            self.refresh_feed().await;
        }
        Ok(Response::new(ContainResponse { containment: Some(containment_to_wire(c)) }))
    }

    async fn request_lift(&self, req: Request<RequestLiftRequest>) -> Result<Response<RequestLiftResponse>, Status> {
        let req = req.into_inner();
        let requested_by = actor(req.requested_by_actor, "", "requested_by_actor")?;
        let reason = req.reason.trim();
        if reason.is_empty() {
            return Err(Status::invalid_argument("reason is required"));
        }
        let c = self.store.containment(qid::<ContainmentId>(&req.containment_qid)?).await.map_err(status)?;
        if c.lifted_at.is_some() {
            return Err(Status::failed_precondition(format!("containment {} was already lifted", c.id)));
        }
        self.store.notify_lift_requested(&c, &requested_by, reason).await.map_err(status)?;
        tracing::info!(containment_id = %c.id, requested_by = %requested_by, "lift requested");
        Ok(Response::new(RequestLiftResponse { containment: Some(containment_to_wire(c)) }))
    }

    async fn lift(&self, req: Request<LiftRequest>) -> Result<Response<LiftResponse>, Status> {
        let req = req.into_inner();
        let lifted_by = actor(req.lifted_by_actor, &req.lifted_by, "lifted_by_actor")?;
        let c = self
            .store
            .lift(qid::<ContainmentId>(&req.containment_qid)?, &lifted_by, &req.reason)
            .await
            .map_err(status)?;
        tracing::warn!(containment_id = %c.id, principal = ?c.principal, lifted_by = %lifted_by, "containment lifted");
        self.refresh_feed().await;
        Ok(Response::new(LiftResponse { containment: Some(containment_to_wire(c)) }))
    }

    async fn list_containments(
        &self,
        req: Request<ListContainmentsRequest>,
    ) -> Result<Response<ListContainmentsResponse>, Status> {
        let req = req.into_inner();
        let principal: Option<EntityRef> = req.principal.map(Into::into);
        let list = self
            .store
            .containments(req.active_only, principal.as_ref(), limit(req.limit), req.include_monitor)
            .await
            .map_err(status)?;
        Ok(Response::new(ListContainmentsResponse {
            containments: list.into_iter().map(containment_to_wire).collect(),
        }))
    }

    async fn list_security_keys(
        &self,
        req: Request<ListSecurityKeysRequest>,
    ) -> Result<Response<ListSecurityKeysResponse>, Status> {
        let human = required(req.into_inner().human_id, "human_id")?;
        let keys = self.keys()?.list(&human).await.map_err(key_status)?;
        Ok(Response::new(ListSecurityKeysResponse { keys: keys.into_iter().map(key_to_wire).collect() }))
    }

    async fn begin_key_change(
        &self,
        req: Request<BeginKeyChangeRequest>,
    ) -> Result<Response<BeginKeyChangeResponse>, Status> {
        let human = required(req.into_inner().human_id, "human_id")?;
        let c = self.keys()?.begin_key_change(&human).await.map_err(key_status)?;
        Ok(Response::new(BeginKeyChangeResponse {
            ceremony_qid: c.id.to_string(),
            options_json: c.options.to_string(),
        }))
    }

    async fn begin_key_registration(
        &self,
        req: Request<BeginKeyRegistrationRequest>,
    ) -> Result<Response<BeginKeyRegistrationResponse>, Status> {
        let req = req.into_inner();
        let human = required(req.human_id, "human_id")?;
        let authorization = req.authorization.map(assertion);
        let c = self.keys()?.begin_registration(&human, authorization.as_ref()).await.map_err(key_status)?;
        Ok(Response::new(BeginKeyRegistrationResponse {
            ceremony_qid: c.id.to_string(),
            options_json: c.options.to_string(),
        }))
    }

    async fn finish_key_registration(
        &self,
        req: Request<FinishKeyRegistrationRequest>,
    ) -> Result<Response<FinishKeyRegistrationResponse>, Status> {
        let req = req.into_inner();
        let human = required(req.human_id, "human_id")?;
        let key = self
            .keys()?
            .finish_registration(&human, &req.ceremony_qid, &req.name, &req.client_data_json, &req.attestation_object)
            .await
            .map_err(key_status)?;
        tracing::warn!(key_id = %key.id, human = %human, added_with = ?key.added_with, "security key added");
        Ok(Response::new(FinishKeyRegistrationResponse { key: Some(key_to_wire(key)) }))
    }

    async fn remove_security_key(
        &self,
        req: Request<RemoveSecurityKeyRequest>,
    ) -> Result<Response<RemoveSecurityKeyResponse>, Status> {
        let req = req.into_inner();
        let human = required(req.human_id, "human_id")?;
        let authorization =
            req.authorization.map(assertion).ok_or_else(|| Status::invalid_argument("authorization is required"))?;
        let key = self.keys()?.remove(&human, qid(&req.security_key_qid)?, &authorization).await.map_err(key_status)?;
        tracing::warn!(key_id = %key.id, human = %human, "security key removed");
        Ok(Response::new(RemoveSecurityKeyResponse { key: Some(key_to_wire(key)) }))
    }

    async fn reset_security_key(
        &self,
        req: Request<ResetSecurityKeyRequest>,
    ) -> Result<Response<ResetSecurityKeyResponse>, Status> {
        let req = req.into_inner();
        let removed_by = actor(req.removed_by_actor, &req.removed_by, "removed_by_actor")?;
        let key = self.keys()?.reset(qid(&req.security_key_qid)?, &removed_by).await.map_err(key_status)?;
        tracing::warn!(key_id = %key.id, human = %key.human_id, removed_by = %removed_by, "security key reset by an admin");
        Ok(Response::new(ResetSecurityKeyResponse { key: Some(key_to_wire(key)) }))
    }

    async fn begin_verification(
        &self,
        req: Request<BeginVerificationRequest>,
    ) -> Result<Response<BeginVerificationResponse>, Status> {
        let req = req.into_inner();
        let human = required(req.human_id, "human_id")?;
        let c = self.keys()?.begin_verification(&human, qid(&req.challenge_qid)?).await.map_err(key_status)?;
        Ok(Response::new(BeginVerificationResponse {
            ceremony_qid: c.id.to_string(),
            options_json: c.options.to_string(),
        }))
    }

    async fn finish_verification(
        &self,
        req: Request<FinishVerificationRequest>,
    ) -> Result<Response<FinishVerificationResponse>, Status> {
        let req = req.into_inner();
        let human = required(req.human_id, "human_id")?;
        let a = req.assertion.map(assertion).ok_or_else(|| Status::invalid_argument("assertion is required"))?;
        let c = self.keys()?.finish_verification(&human, &a).await.map_err(key_status)?;
        tracing::info!(challenge_id = %c.id, human = %human, status = c.status.as_str(), "challenge verified with a security key");
        Ok(Response::new(FinishVerificationResponse { challenge: Some(challenge_to_wire(c)) }))
    }

    type WatchChallengesStream = ChallengeStream;

    async fn watch_challenges(
        &self,
        req: Request<WatchChallengesRequest>,
    ) -> Result<Response<Self::WatchChallengesStream>, Status> {
        let answerer = Some(req.into_inner().answerer_id).filter(|a| !a.is_empty());
        let mut rx = self.feed.challenges();
        let store = self.store.clone();
        let (tx, out) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            loop {
                let next = tokio::select! {
                    id = rx.recv() => id,
                    _ = tokio::time::sleep(CHALLENGE_HEARTBEAT) => {
                        if tx.send(Ok(WatchChallengesResponse { challenge: None })).await.is_err() { return }
                        continue;
                    }
                };
                let id = match next {
                    Ok(id) => id,
                    // Fell behind: the watcher's next list catches it up.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                };
                let Ok(c) = store.get(id).await else { continue };
                if answerer.as_ref().is_some_and(|a| c.answerer_id.as_ref() != Some(a)) {
                    continue;
                }
                if tx.send(Ok(WatchChallengesResponse { challenge: Some(challenge_to_wire(c)) })).await.is_err() {
                    return; // the watcher went away
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(out))))
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
    fn keys(&self) -> Result<&SecurityKeys, Status> {
        self.keys.as_deref().ok_or_else(|| {
            Status::failed_precondition(
                "security keys are off: awzy-challenge has no WebAuthn relying party (AWZY_CHALLENGE_WEBAUTHN_RP_ID)",
            )
        })
    }

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

fn required(s: String, what: &str) -> Result<String, Status> {
    if s.is_empty() {
        Err(Status::invalid_argument(format!("{what} is required")))
    } else {
        Ok(s)
    }
}

fn assertion(a: WireAssertion) -> Assertion {
    Assertion {
        ceremony_id: a.ceremony_qid,
        credential_id: a.credential_id,
        client_data_json: a.client_data_json,
        authenticator_data: a.authenticator_data,
        signature: a.signature,
    }
}

fn key_status(e: KeyError) -> Status {
    match e {
        KeyError::Challenge(e) => status(e),
        KeyError::WebAuthn(_) | KeyError::UnknownKey { .. } | KeyError::NeedsExistingKey { .. } => {
            Status::permission_denied(e.to_string())
        }
        KeyError::NoKeys { .. } | KeyError::BadCeremony => Status::failed_precondition(e.to_string()),
        KeyError::NotFound(_) => Status::not_found(e.to_string()),
        KeyError::AlreadyRegistered => Status::already_exists(e.to_string()),
        KeyError::Invalid(_) => Status::invalid_argument(e.to_string()),
        KeyError::Db(error) => {
            tracing::error!(%error, "security key store failed");
            Status::internal("challenge store unavailable")
        }
    }
}

fn key_to_wire(k: SecurityKey) -> WireKey {
    let (removed_by_actor, removed_by) = awzy_authz_proto::actor_to_wire(k.removed_by.as_deref().unwrap_or(""));
    WireKey {
        qid: k.id.to_string(),
        human_id: k.human_id,
        name: k.name,
        added_with_qid: k.added_with.map(|id| id.to_string()).unwrap_or_default(),
        created_at: Some(ts(k.created_at)),
        last_used_at: k.last_used_at.map(ts),
        removed_at: k.removed_at.map(ts),
        removed_by,
        removed_by_actor,
    }
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

fn support_status(e: SupportError) -> Status {
    match e {
        SupportError::NotFound(_) => Status::not_found(e.to_string()),
        SupportError::Invalid(_) => Status::invalid_argument(e.to_string()),
        SupportError::WrongStatus { .. } => Status::failed_precondition(e.to_string()),
        SupportError::Db(error) => {
            tracing::error!(%error, "support store failed");
            Status::internal("challenge store unavailable")
        }
    }
}

/// Who acted, stored as text in its typed form (ADR 0060).
fn actor(actor: Option<awzy_authz_proto::authz::v1::Actor>, text: &str, field: &str) -> Result<String, Status> {
    awzy_authz_proto::actor_from_wire(actor, text)
        .map(|a| a.to_string())
        .ok_or_else(|| Status::invalid_argument(format!("{field} is required")))
}

/// The person who answered or decided, by their IdP ID: only a person, acting for themselves,
/// answers for a challenge, and approvals are checked against them as a Cedar principal.
fn human(actor: Option<awzy_authz_proto::authz::v1::Actor>, text: &str, field: &str) -> Result<String, Status> {
    match awzy_authz_proto::actor_from_wire(actor, text) {
        Some(a) if a.subject.kind == awzy_model::entity_type::HUMAN && a.on_behalf_of.is_none() => Ok(a.subject.id),
        Some(a) => Err(Status::invalid_argument(format!("{field} must be a person acting for themselves, not {a}"))),
        None => Err(Status::invalid_argument(format!("{field} is required"))),
    }
}

/// Who may approve: a person, or a service account such as an overseer agent (ADR 0065), acting on
/// its own authority. Policy decides whether it may approve this resource.
fn approver(actor: Option<awzy_authz_proto::authz::v1::Actor>, text: &str) -> Result<awzy_model::Actor, Status> {
    use awzy_model::entity_type::{HUMAN, SERVICE_ACCOUNT};
    match awzy_authz_proto::actor_from_wire(actor, text) {
        Some(a) if [HUMAN, SERVICE_ACCOUNT].contains(&a.subject.kind.as_str()) && a.on_behalf_of.is_none() => Ok(a),
        Some(a) => Err(Status::invalid_argument(format!(
            "decided_by_actor must be a person or a service account acting on its own authority, not {a}"
        ))),
        None => Err(Status::invalid_argument("decided_by_actor is required")),
    }
}

fn ts(t: DateTime<Utc>) -> Timestamp {
    Timestamp { seconds: t.timestamp(), nanos: t.timestamp_subsec_nanos() as i32 }
}

fn challenge_to_wire(c: Challenge) -> WireChallenge {
    let (decided_by_actor, decided_by) = awzy_authz_proto::actor_to_wire(c.decided_by.as_deref().unwrap_or(""));
    WireChallenge {
        qid: c.id.to_string(),
        decision_qid: c.decision_id.to_string(),
        principal: Some(c.request.principal.into()),
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
        decided_by,
        decided_by_actor,
        decision_note: c.decision_note.unwrap_or_default(),
        created_at: Some(ts(c.created_at)),
        expires_at: Some(ts(c.expires_at)),
        passed_at: c.passed_at.map(ts),
        grant_expires_at: c.grant_expires_at.map(ts),
    }
}

fn containment_to_wire(c: Containment) -> WireContainment {
    let (created_by_actor, created_by) = awzy_authz_proto::actor_to_wire(&c.created_by);
    let (lifted_by_actor, lifted_by) = awzy_authz_proto::actor_to_wire(c.lifted_by.as_deref().unwrap_or(""));
    WireContainment {
        qid: c.id.to_string(),
        principal: Some(c.principal.into()),
        reason: c.reason,
        created_by,
        created_by_actor,
        created_at: Some(ts(c.created_at)),
        lifted_at: c.lifted_at.map(ts),
        lifted_by,
        lifted_by_actor,
        lift_reason: c.lift_reason.unwrap_or_default(),
        monitor: c.monitor,
    }
}

/// End to end against Postgres: an awzy-pdp in enforce mode opens challenges here, honors them
/// once passed, and follows the containment feed.
#[cfg(test)]
mod tests {
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
        let seen =
            tokio::time::timeout(Duration::from_secs(5), bobs.message()).await.expect("streamed").unwrap().unwrap();
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
        let for_someone =
            client.decide(decide(awzy_model::Actor::new(overseer.clone(), None).for_human("carol"))).await;
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

        let list =
            |include_monitor| ListContainmentsRequest { active_only: true, include_monitor, ..Default::default() };
        let listed = |r: tonic::Response<ListContainmentsResponse>| r.into_inner().containments.len();
        assert_eq!(listed(client.list_containments(list(false)).await.unwrap()), 0, "left out unless asked for");
        assert_eq!(listed(client.list_containments(list(true)).await.unwrap()), 1);
    }
}

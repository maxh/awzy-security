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
use awzy_model::{ContainmentId, DecisionId, EntityRef, PolicyBundleId, SupportSessionId};
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

mod convert;

use convert::*;

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
        rules.extend(group(&["Justify", "Decide"], &self.answerers));
        // Reading one challenge: the portal, and the PDPs for an agent waiting on its own (ADR 0111).
        rules.extend(group(&["GetChallenge"], &either(&self.answerers, &self.pdps)));
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
        // Optional: a break-glass restore names the bundle only when it restored one.
        if !req.policy_bundle_qid.is_empty() {
            qid::<PolicyBundleId>(&req.policy_bundle_qid)?;
        }
        let rollback = crate::notify::BreakGlass {
            key_id: req.key_id,
            reason: req.reason,
            restored_version: req.restored_version,
            policy_bundle_qid: req.policy_bundle_qid,
            version: req.version,
        };
        self.store.notify_break_glass(rollback).await.map_err(status)?;
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
        let authorization = req.authorization.map(assertion).transpose()?;
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
            .finish_registration(
                &human,
                qid(&req.ceremony_qid)?,
                &req.name,
                &req.client_data_json,
                &req.attestation_object,
            )
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
        let authorization = req
            .authorization
            .map(assertion)
            .transpose()?
            .ok_or_else(|| Status::invalid_argument("authorization is required"))?;
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
        let a = req
            .assertion
            .map(assertion)
            .transpose()?
            .ok_or_else(|| Status::invalid_argument("assertion is required"))?;
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

/// End to end against Postgres: an awzy-pdp in enforce mode opens challenges here, honors them
/// once passed, and follows the containment feed.
#[cfg(test)]
mod tests;

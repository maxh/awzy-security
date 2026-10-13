//! `awzy.policy.v1.PolicyService` over gRPC.

use awzy_service_auth::{require_by_path, Allowed, PathGuard, Rule};
use std::sync::Arc;

use awzy_authz_proto::{
    policy::v1::{
        get_bundle_request::Key,
        policy_service_server::{PolicyService, PolicyServiceServer},
        BreakGlassRollback as WireBreakGlass, BreakGlassRollbackRequest, BreakGlassRollbackResponse,
        BundleStatus as WireStatus, CreateDraftRequest,
        CreateDraftResponse, EnforcementSetting as WireSetting, GetActiveRequest, GetActiveResponse, GetBundleRequest,
        GetBundleResponse, GetEnforcementRequest, GetEnforcementResponse, GetModelRoutingRequest,
        GetModelRoutingResponse, GetOutageFallbackRequest, GetOutageFallbackResponse, ListBundlesRequest,
        ListBreakGlassRollbacksRequest, ListBreakGlassRollbacksResponse, ListBundlesResponse,
        ListEnforcementChangesRequest, ListEnforcementChangesResponse,
        ListEnforcementSettingsRequest, ListEnforcementSettingsResponse, ModelRouting as WireRouting,
        OutageFallback as WireFallback, PolicyBundle as WireBundle, PublishRequest, PublishResponse,
        SetEnforcementRequest, SetEnforcementResponse, SetModelRoutingRequest, SetModelRoutingResponse,
        SetOutageFallbackRequest, SetOutageFallbackResponse,
    },
    prost_types::Timestamp,
};
use awzy_model::{OutageFallback, PolicyBundleId};
use axum::Router;
use chrono::{DateTime, Utc};
use tonic::{Request, Response, Status};

use awzy_signing::{break_glass_payload, BundleVerifier};

use crate::{
    BreakGlassRequest, BundleStatus, EnforcementSetting, ModelRoutingSetting, OutageFallbackSetting, PolicyBundle,
    PolicyStore, StoreError, INSTALL_SCOPE,
};

/// What readers may call. A method in no list is refused for everyone, so every one the service
/// has must be in one (a test checks against policy.proto).
const READ_METHODS: &[&str] = &[
    "GetActive",
    "GetBundle",
    "GetEnforcement",
    "GetModelRouting",
    "GetOutageFallback",
    "ListBreakGlassRollbacks",
    "ListBundles",
    "ListEnforcementChanges",
    "ListEnforcementSettings",
];

/// What writers may call.
const WRITE_METHODS: &[&str] = &["CreateDraft", "Publish", "SetEnforcement", "SetModelRouting", "SetOutageFallback"];

/// Signed by enrolled admin keys instead, and needed when everything else is broken (ADR 0022).
const EXEMPT_METHODS: &[&str] = &["BreakGlassRollback"];

/// Who may call which methods (ADR 0055). Each group's callers come from the chart, as
/// `namespace/service-account`; a method no group names is refused.
pub struct Access {
    verifier: Arc<awzy_service_auth::Verifier>,
    /// Reading bundles, the mode, the outage fallback, and the model routing: the PDP, the client
    /// portal, awzy-gateway, and awzy-reasoner.
    readers: Allowed,
    /// Drafting, publishing, switching the mode, and setting the outage fallback and the model
    /// routing: the client portal.
    writers: Allowed,
}

impl Access {
    /// From `AWZY_POLICY_READERS` and `AWZY_POLICY_WRITERS`.
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            verifier: Arc::new(awzy_service_auth::Verifier::from_env("awzy-policy")?),
            readers: Allowed::from_env("AWZY_POLICY_READERS"),
            writers: Allowed::from_env("AWZY_POLICY_WRITERS"),
        })
    }

    /// Everyone may call everything: for tests and local runs.
    pub fn off() -> Self {
        Self {
            verifier: Arc::new(awzy_service_auth::Verifier::off()),
            readers: Allowed::only(&[]),
            writers: Allowed::only(&[]),
        }
    }

    fn guard(&self) -> PathGuard {
        let method = |m: &str| format!("/awzy.policy.v1.PolicyService/{m}");
        let mut rules: Vec<(String, Rule)> =
            READ_METHODS.iter().map(|m| (method(m), Rule::Allow(self.readers.clone()))).collect();
        rules.extend(WRITE_METHODS.iter().map(|m| (method(m), Rule::Allow(self.writers.clone()))));
        rules.extend(EXEMPT_METHODS.iter().map(|m| (method(m), Rule::Exempt)));
        PathGuard::new(self.verifier.clone(), rules)
    }
}

/// awzy-challenge, told of each break-glass rollback so the security team hears of it.
pub type BreakGlassReporter =
    awzy_authz_proto::challenge::v1::challenge_service_client::ChallengeServiceClient<awzy_service_auth::AuthChannel>;

/// `break_glass`: the public keys enrolled at install time for break-glass rollback. None turns
/// the RPC off. `reporter`: who to tell of each rollback. `access`: who may call which methods.
pub fn router(
    store: Arc<PolicyStore>,
    break_glass: Option<Arc<BundleVerifier>>,
    reporter: Option<BreakGlassReporter>,
    access: &Access,
) -> Router {
    tonic::service::Routes::new(PolicyServiceServer::new(Service(store, break_glass, reporter)))
        .into_axum_router()
        .route_layer(axum::middleware::from_fn_with_state(access.guard(), require_by_path))
}

/// How far a break-glass request's timestamp may be from now.
const BREAK_GLASS_WINDOW_SECS: i64 = 300;

struct Service(Arc<PolicyStore>, Option<Arc<BundleVerifier>>, Option<BreakGlassReporter>);

#[tonic::async_trait]
impl PolicyService for Service {
    async fn create_draft(&self, req: Request<CreateDraftRequest>) -> Result<Response<CreateDraftResponse>, Status> {
        let req = req.into_inner();
        let by = actor(req.created_by_actor, &req.created_by, "created_by_actor")?;
        let bundle = self.0.create_draft(&req.schema, &req.policies, &by).await.map_err(status)?;
        Ok(Response::new(CreateDraftResponse { bundle: Some(to_wire(bundle)) }))
    }

    async fn publish(&self, req: Request<PublishRequest>) -> Result<Response<PublishResponse>, Status> {
        let req = req.into_inner();
        let id: PolicyBundleId =
            req.policy_bundle_qid.parse().map_err(|e: awzy_model::QidError| Status::invalid_argument(e.to_string()))?;
        let by = actor(req.published_by_actor, &req.published_by, "published_by_actor")?;
        let bundle = self.0.publish(id, &by).await.map_err(status)?;
        Ok(Response::new(PublishResponse { bundle: Some(to_wire(bundle)) }))
    }

    async fn get_active(&self, req: Request<GetActiveRequest>) -> Result<Response<GetActiveResponse>, Status> {
        let req = req.into_inner();
        let known = req.known_version;
        // Taken before reading, so a publish committed in between still wakes the wait.
        let seen = self.0.changed().current();
        let mut active = self.0.active().await.map_err(status)?;
        let unchanged = |a: &Option<crate::store::PolicyBundle>| matches!(a, Some(b) if b.version == known);
        if unchanged(&active) && req.wait_seconds > 0 {
            // Wakes for a new mode too, so the caller reads it straight after (ADR 0077).
            let store = self.0.clone();
            // The install's mode as it was, so a change to it also ends the wait.
            // Times are whole seconds, so the mode and who set it tell apart two changes in one.
            let setting = |e: crate::store::EnforcementSetting| (e.changed_at, e.mode, e.changed_by);
            let mode_then = store.enforcement(crate::store::INSTALL_SCOPE).await.ok().map(setting);
            let check = || {
                let store = store.clone();
                let mode_then = mode_then.clone();
                async move {
                    let bundle_moved = !matches!(store.active().await, Ok(Some(b)) if b.version == known);
                    bundle_moved || store.enforcement(crate::store::INSTALL_SCOPE).await.ok().map(setting) != mode_then
                }
            };
            self.0
                .changed()
                .wait_until(seen, awzy_store::changes::wait_for(Some(req.wait_seconds.into())), check)
                .await;
            active = self.0.active().await.map_err(status)?;
        }
        let res = match active {
            Some(b) if b.version == known => GetActiveResponse { bundle: None, unchanged: true },
            other => GetActiveResponse { bundle: other.map(to_wire), unchanged: false },
        };
        Ok(Response::new(res))
    }

    async fn list_bundles(&self, req: Request<ListBundlesRequest>) -> Result<Response<ListBundlesResponse>, Status> {
        let limit = match req.into_inner().limit {
            n if n <= 0 => 50,
            n => n.min(500),
        };
        let bundles = self.0.list(limit.into()).await.map_err(status)?;
        Ok(Response::new(ListBundlesResponse { bundles: bundles.into_iter().map(to_wire).collect() }))
    }

    async fn get_bundle(&self, req: Request<GetBundleRequest>) -> Result<Response<GetBundleResponse>, Status> {
        let bundle = match req.into_inner().key {
            Some(Key::PolicyBundleQid(id)) => {
                let id: PolicyBundleId =
                    id.parse().map_err(|e: awzy_model::QidError| Status::invalid_argument(e.to_string()))?;
                self.0.get(id).await.map_err(status)?
            }
            Some(Key::Version(v)) => self
                .0
                .get_by_version(v)
                .await
                .map_err(status)?
                .ok_or_else(|| Status::not_found(format!("no policy bundle has version {v}")))?,
            None => return Err(Status::invalid_argument("id or version is required")),
        };
        Ok(Response::new(GetBundleResponse { bundle: Some(to_wire(bundle)) }))
    }

    async fn get_enforcement(
        &self,
        req: Request<GetEnforcementRequest>,
    ) -> Result<Response<GetEnforcementResponse>, Status> {
        let scope = scope_or_install(req.into_inner().scope);
        let setting = self.0.enforcement(&scope).await.map_err(status)?;
        Ok(Response::new(GetEnforcementResponse { setting: Some(setting_to_wire(setting)) }))
    }

    async fn set_enforcement(
        &self,
        req: Request<SetEnforcementRequest>,
    ) -> Result<Response<SetEnforcementResponse>, Status> {
        let req = req.into_inner();
        let mode = match req.mode() {
            awzy_authz_proto::authz::v1::EnforcementMode::Unspecified => {
                return Err(Status::invalid_argument("mode is required"))
            }
            m => m.into(),
        };
        let by = actor(req.changed_by_actor.clone(), &req.changed_by, "changed_by_actor")?;
        let setting = self.0.set_enforcement(&scope_or_install(req.scope), mode, &by).await.map_err(status)?;
        tracing::info!(scope = %setting.scope, mode = setting.mode.as_str(), changed_by = ?setting.changed_by, "enforcement mode changed");
        Ok(Response::new(SetEnforcementResponse { setting: Some(setting_to_wire(setting)) }))
    }

    async fn list_enforcement_changes(
        &self,
        req: Request<ListEnforcementChangesRequest>,
    ) -> Result<Response<ListEnforcementChangesResponse>, Status> {
        let req = req.into_inner();
        let limit = if req.limit <= 0 { 20 } else { req.limit.min(200) };
        let changes = self.0.enforcement_changes(&scope_or_install(req.scope), limit.into()).await.map_err(status)?;
        Ok(Response::new(ListEnforcementChangesResponse {
            changes: changes.into_iter().map(setting_to_wire).collect(),
        }))
    }

    async fn list_break_glass_rollbacks(
        &self,
        req: Request<ListBreakGlassRollbacksRequest>,
    ) -> Result<Response<ListBreakGlassRollbacksResponse>, Status> {
        let limit = match req.into_inner().limit {
            n if n <= 0 => 50,
            n => n.min(500),
        };
        let events = self.0.break_glass_events(limit.into()).await.map_err(status)?;
        let ts = |t: DateTime<Utc>| Timestamp { seconds: t.timestamp(), nanos: t.timestamp_subsec_nanos() as i32 };
        let rollbacks = events
            .into_iter()
            .map(|e| WireBreakGlass {
                key_id: e.key_id,
                reason: e.reason,
                requested_at: Some(ts(e.requested_at)),
                recorded_at: Some(ts(e.recorded_at)),
                restored_version: e.restored_version,
                replaced_version: e.replaced_version.unwrap_or_default(),
                bundle_qid: e.bundle_id.to_string(),
            })
            .collect();
        Ok(Response::new(ListBreakGlassRollbacksResponse { rollbacks }))
    }

    async fn list_enforcement_settings(
        &self,
        _: Request<ListEnforcementSettingsRequest>,
    ) -> Result<Response<ListEnforcementSettingsResponse>, Status> {
        let settings = self.0.scoped_settings().await.map_err(status)?;
        Ok(Response::new(ListEnforcementSettingsResponse {
            settings: settings.into_iter().map(setting_to_wire).collect(),
        }))
    }

    async fn get_outage_fallback(
        &self,
        req: Request<GetOutageFallbackRequest>,
    ) -> Result<Response<GetOutageFallbackResponse>, Status> {
        let known = req.into_inner().known_version;
        let setting = self.0.outage_fallback().await.map_err(status)?;
        if setting.version == known {
            return Ok(Response::new(GetOutageFallbackResponse { fallback: None, unchanged: true }));
        }
        Ok(Response::new(GetOutageFallbackResponse { fallback: Some(fallback_to_wire(setting)), unchanged: false }))
    }

    async fn set_outage_fallback(
        &self,
        req: Request<SetOutageFallbackRequest>,
    ) -> Result<Response<SetOutageFallbackResponse>, Status> {
        let req = req.into_inner();
        let by = actor(req.changed_by_actor.clone(), "", "changed_by_actor")?;
        let fallback = OutageFallback {
            tools: req.tools.into_iter().collect(),
            servers: req.servers.into_iter().collect(),
            fail_open_tiers: req.fail_open_tiers.into_iter().collect(),
        };
        let setting = self.0.set_outage_fallback(&fallback, &by).await.map_err(status)?;
        tracing::info!(version = setting.version, changed_by = %by, fail_open = ?setting.fallback.fail_open_tiers, "outage fallback changed");
        Ok(Response::new(SetOutageFallbackResponse { fallback: Some(fallback_to_wire(setting)) }))
    }

    async fn get_model_routing(
        &self,
        req: Request<GetModelRoutingRequest>,
    ) -> Result<Response<GetModelRoutingResponse>, Status> {
        let known = req.into_inner().known_version;
        let setting = self.0.model_routing().await.map_err(status)?;
        if setting.version == known {
            return Ok(Response::new(GetModelRoutingResponse { routing: None, unchanged: true }));
        }
        Ok(Response::new(GetModelRoutingResponse { routing: Some(routing_to_wire(setting)), unchanged: false }))
    }

    async fn set_model_routing(
        &self,
        req: Request<SetModelRoutingRequest>,
    ) -> Result<Response<SetModelRoutingResponse>, Status> {
        let req = req.into_inner();
        let by = actor(req.changed_by_actor.clone(), "", "changed_by_actor")?;
        let setting = self.0.set_model_routing(&req.routing_json, &by).await.map_err(status)?;
        tracing::info!(version = setting.version, changed_by = %by, "model routing changed");
        Ok(Response::new(SetModelRoutingResponse { routing: Some(routing_to_wire(setting)) }))
    }

    async fn break_glass_rollback(
        &self,
        req: Request<BreakGlassRollbackRequest>,
    ) -> Result<Response<BreakGlassRollbackResponse>, Status> {
        let Some(keys) = &self.1 else {
            return Err(Status::failed_precondition("break-glass is off: no keys were enrolled at install time"));
        };
        let req = req.into_inner();
        if req.reason.trim().is_empty() || req.nonce.is_empty() {
            return Err(Status::invalid_argument("reason and nonce are required"));
        }
        let now = Utc::now().timestamp();
        if (now - req.requested_at_unix).abs() > BREAK_GLASS_WINDOW_SECS {
            return Err(Status::permission_denied("break-glass request is too old or from the future; sign a new one"));
        }
        let payload = break_glass_payload(req.to_version, req.requested_at_unix, &req.nonce, &req.reason);
        if let Err(e) = keys.verify_payload(&payload, &req.signature, &req.key_id) {
            tracing::error!(key_id = %req.key_id, error = %e, "rejected break-glass request");
            return Err(Status::permission_denied(e.to_string()));
        }
        let verified = BreakGlassRequest {
            key_id: req.key_id.clone(),
            nonce: req.nonce,
            reason: req.reason.clone(),
            requested_at: DateTime::from_timestamp(req.requested_at_unix, 0).unwrap_or_else(Utc::now),
            to_version: (req.to_version > 0).then_some(req.to_version),
        };
        let out = self.0.break_glass_rollback(&verified).await.map_err(status)?;
        // The security team hears of every use. The rollback stands either way.
        match self.2.clone() {
            Some(mut reporter) => {
                let report = awzy_authz_proto::challenge::v1::ReportBreakGlassRequest {
                    key_id: req.key_id.clone(),
                    reason: req.reason.clone(),
                    restored_version: out.restored_version,
                    policy_bundle_qid: out.bundle.id.to_string(),
                    version: out.bundle.version,
                };
                if let Err(e) = reporter.report_break_glass(report).await {
                    tracing::error!(error = %e.message(), "couldn't tell the security team about a break-glass rollback");
                }
            }
            None => tracing::warn!("AWZY_CHALLENGE_ADDR is not set; nobody is told about this break-glass rollback"),
        }
        tracing::error!(
            key_id = %req.key_id,
            reason = %req.reason,
            restored_version = out.restored_version,
            replaced_version = ?out.replaced_version,
            new_version = out.bundle.version,
            "BREAK-GLASS: policy bundle rolled back"
        );
        Ok(Response::new(BreakGlassRollbackResponse {
            restored_version: out.restored_version,
            replaced_version: out.replaced_version.unwrap_or(0),
            bundle: Some(to_wire(out.bundle)),
        }))
    }
}

fn status(e: StoreError) -> Status {
    match e {
        StoreError::Invalid(_) => Status::invalid_argument(e.to_string()),
        StoreError::NotFound(_) => Status::not_found(e.to_string()),
        StoreError::NotDraft { .. } => Status::failed_precondition(e.to_string()),
        StoreError::UnknownScope(_) => Status::invalid_argument(e.to_string()),
        StoreError::InvalidFallback(_) => Status::invalid_argument(e.to_string()),
        StoreError::Signing(error) => {
            tracing::error!(%error, "signing failed");
            Status::unavailable("awzy-policy couldn't sign; nothing changed")
        }
        StoreError::NonceReused => Status::permission_denied(e.to_string()),
        StoreError::NoRollbackTarget(_) => Status::failed_precondition(e.to_string()),
        StoreError::Db(error) => {
            tracing::error!(%error, "policy store failed");
            Status::internal("policy store unavailable")
        }
    }
}

fn scope_or_install(scope: String) -> String {
    if scope.is_empty() {
        INSTALL_SCOPE.to_string()
    } else {
        scope
    }
}

/// Who acted, stored as text in its typed form (ADR 0060). Every change names someone.
fn actor(actor: Option<awzy_authz_proto::authz::v1::Actor>, text: &str, field: &str) -> Result<String, Status> {
    awzy_authz_proto::actor_from_wire(actor, text)
        .map(|a| a.to_string())
        .ok_or_else(|| Status::invalid_argument(format!("{field} is required")))
}

fn setting_to_wire(s: EnforcementSetting) -> WireSetting {
    let (changed_by_actor, changed_by) = awzy_authz_proto::actor_to_wire(s.changed_by.as_deref().unwrap_or(""));
    WireSetting {
        scope: s.scope,
        mode: awzy_authz_proto::authz::v1::EnforcementMode::from(s.mode).into(),
        changed_by,
        changed_by_actor,
        changed_at: s
            .changed_at
            .map(|t| Timestamp { seconds: t.timestamp(), nanos: t.timestamp_subsec_nanos() as i32 }),
        signature: s.signature.unwrap_or_default(),
        signing_key_id: s.signing_key_id.unwrap_or_default(),
    }
}

fn fallback_to_wire(s: OutageFallbackSetting) -> WireFallback {
    let (changed_by_actor, _) = awzy_authz_proto::actor_to_wire(s.changed_by.as_deref().unwrap_or(""));
    WireFallback {
        version: s.version,
        tools: s.fallback.tools.into_iter().collect(),
        servers: s.fallback.servers.into_iter().collect(),
        fail_open_tiers: s.fallback.fail_open_tiers.into_iter().collect(),
        changed_by_actor,
        changed_at: s
            .changed_at
            .map(|t| Timestamp { seconds: t.timestamp(), nanos: t.timestamp_subsec_nanos() as i32 }),
        signature: s.signature.unwrap_or_default(),
        signing_key_id: s.signing_key_id.unwrap_or_default(),
    }
}

fn routing_to_wire(s: ModelRoutingSetting) -> WireRouting {
    let (changed_by_actor, _) = awzy_authz_proto::actor_to_wire(s.changed_by.as_deref().unwrap_or(""));
    WireRouting {
        version: s.version,
        routing_json: s.routing_json,
        changed_by_actor,
        changed_at: s
            .changed_at
            .map(|t| Timestamp { seconds: t.timestamp(), nanos: t.timestamp_subsec_nanos() as i32 }),
        signature: s.signature.unwrap_or_default(),
        signing_key_id: s.signing_key_id.unwrap_or_default(),
    }
}

fn to_wire(b: PolicyBundle) -> WireBundle {
    let ts = |t: DateTime<Utc>| Timestamp { seconds: t.timestamp(), nanos: t.timestamp_subsec_nanos() as i32 };
    let (created_by_actor, created_by) = awzy_authz_proto::actor_to_wire(&b.created_by);
    let (published_by_actor, published_by) = awzy_authz_proto::actor_to_wire(b.published_by.as_deref().unwrap_or(""));
    WireBundle {
        qid: b.id.to_string(),
        version: b.version,
        schema: b.schema,
        policies: b.policies,
        status: match b.status {
            BundleStatus::Draft => WireStatus::Draft,
            BundleStatus::Active => WireStatus::Active,
            BundleStatus::Superseded => WireStatus::Superseded,
        }
        .into(),
        created_by,
        created_by_actor,
        created_at: Some(ts(b.created_at)),
        published_by,
        published_by_actor,
        published_at: b.published_at.map(ts),
        signature: b.signature.unwrap_or_default(),
        signing_key_id: b.signing_key_id.unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awzy_authz_proto::policy::v1::policy_service_client::PolicyServiceClient;
    use tonic::Code;

    #[test]
    fn every_method_the_service_has_is_in_an_access_list() {
        // A method in no list is refused for every caller: ListEnforcementSettings once was, and
        // the Enforcement page couldn't load.
        let proto = include_str!("../../../libs/proto/awzy/policy/v1/policy.proto");
        let methods: Vec<&str> = proto
            .lines()
            .filter_map(|l| l.trim().strip_prefix("rpc "))
            .filter_map(|l| l.split('(').next())
            .map(str::trim)
            .collect();
        assert!(methods.len() > 10, "read the methods from policy.proto: {methods:?}");
        for m in &methods {
            let lists = [READ_METHODS, WRITE_METHODS, EXEMPT_METHODS];
            assert_eq!(lists.iter().filter(|l| l.contains(m)).count(), 1, "{m} is in exactly one access list");
        }
        for m in READ_METHODS.iter().chain(WRITE_METHODS).chain(EXEMPT_METHODS) {
            assert!(methods.contains(m), "{m} is listed but isn't a method of the service");
        }
    }

    async fn admin_client(endpoint: &str) -> PolicyServiceClient<tonic::transport::Channel> {
        PolicyServiceClient::connect(endpoint.to_string()).await.unwrap()
    }

    async fn admin_draft(c: &mut PolicyServiceClient<tonic::transport::Channel>, policies: &str) -> String {
        c.create_draft(CreateDraftRequest {
            schema: awzy_pdp::DEFAULT_SCHEMA.into(),
            policies: policies.into(),
            created_by: "bob".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .bundle
        .unwrap()
        .qid
    }

    /// Draft, publish, and fetch over gRPC; then an awzy-pdp picks the bundle up.
    #[tokio::test]
    async fn publish_flow_reaches_a_pdp() {
        let Some(store) = crate::store::tests::test_store().await else { return };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let glass_key =
            awzy_signing::BundleSigner::from_key(p256::ecdsa::SigningKey::from_bytes(&[11u8; 32].into()).unwrap())
                .unwrap();
        let enrolled =
            awzy_signing::BundleVerifier::from_keys([*p256::ecdsa::SigningKey::from_bytes(&[11u8; 32].into())
                .unwrap()
                .verifying_key()])
            .unwrap();
        let app = router(Arc::new(store), Some(Arc::new(enrolled)), None, &Access::off());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut client = PolicyServiceClient::connect(endpoint.clone()).await.unwrap();

        let invalid = client
            .create_draft(CreateDraftRequest {
                schema: "nonsense".into(),
                policies: String::new(),
                created_by: "alice".into(),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert_eq!(invalid.code(), Code::InvalidArgument);

        let draft = client
            .create_draft(CreateDraftRequest {
                schema: awzy_pdp::DEFAULT_SCHEMA.into(),
                policies: awzy_pdp::DEFAULT_POLICIES.into(),
                created_by_actor: Some(awzy_model::Actor::awzy("awzy-bot").for_human("alice").into()),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner()
            .bundle
            .unwrap();
        assert_eq!(draft.status(), WireStatus::Draft);
        // The typed actor comes back as sent, and as its text form for callers that predate it.
        assert_eq!(
            awzy_authz_proto::actor_from_wire(draft.created_by_actor.clone(), ""),
            Some(awzy_model::Actor::awzy("awzy-bot").for_human("alice"))
        );
        assert_eq!(draft.created_by, "AwzyService::awzy-bot for Human::alice");
        let active = client
            .publish(PublishRequest {
                policy_bundle_qid: draft.qid.clone(),
                published_by: "alice".into(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner()
            .bundle
            .unwrap();
        assert_eq!(active.status(), WireStatus::Active);
        // A caller still sending text is read as before: a bare ID is a person.
        assert_eq!(active.published_by, "Human::alice");

        let pdp = awzy_pdp::shared(awzy_pdp::Pdp::with_defaults().unwrap());
        let enforcement = awzy_pdp::Enforcement::awaiting_sync();
        let mut pdp_client = None;
        assert!(awzy_pdp::sync_once(&pdp, &enforcement, None, &endpoint, &mut pdp_client).await.unwrap());
        assert_eq!(pdp.load().version(), active.version);
        assert_eq!(enforcement.mode(), awzy_model::EnforcementMode::Monitor, "a new install starts in monitor");

        let mut admin = PolicyServiceClient::connect(endpoint.clone()).await.unwrap();
        admin
            .set_enforcement(SetEnforcementRequest {
                scope: String::new(),
                mode: awzy_authz_proto::authz::v1::EnforcementMode::Enforce.into(),
                changed_by: "alice".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        awzy_pdp::sync_once(&pdp, &enforcement, None, &endpoint, &mut pdp_client).await.unwrap();
        assert_eq!(enforcement.mode(), awzy_model::EnforcementMode::Enforce);

        // Break-glass: a bad bundle goes out, an enrolled admin rolls it back.
        let bad = admin_draft(&mut admin_client(&endpoint).await, "forbid (principal, action, resource);").await;
        admin_client(&endpoint)
            .await
            .publish(PublishRequest { policy_bundle_qid: bad, published_by: "bob".into(), ..Default::default() })
            .await
            .unwrap();
        let signed = |to_version: i64, nonce: &str, at: i64, signer: &awzy_signing::BundleSigner| {
            let reason = "forbid-all locked everyone out".to_string();
            BreakGlassRollbackRequest {
                to_version,
                requested_at_unix: at,
                nonce: nonce.into(),
                signature: signer.sign_payload(&awzy_signing::break_glass_payload(to_version, at, nonce, &reason)),
                key_id: signer.key_id().into(),
                reason,
            }
        };
        let now = Utc::now().timestamp();
        let restored = admin_client(&endpoint)
            .await
            .break_glass_rollback(signed(0, "n-1", now, &glass_key))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(restored.restored_version, active.version);
        assert_eq!(restored.bundle.unwrap().published_by, format!("BreakGlassKey::{}", glass_key.key_id()));

        let mut c = admin_client(&endpoint).await;
        let replay = c.break_glass_rollback(signed(0, "n-1", now, &glass_key)).await.unwrap_err();
        assert_eq!(replay.code(), Code::PermissionDenied);
        let stale = c.break_glass_rollback(signed(0, "n-2", now - 3600, &glass_key)).await.unwrap_err();
        assert_eq!(stale.code(), Code::PermissionDenied);
        let stranger =
            awzy_signing::BundleSigner::from_key(p256::ecdsa::SigningKey::from_bytes(&[12u8; 32].into()).unwrap())
                .unwrap();
        let unknown = c.break_glass_rollback(signed(0, "n-3", now, &stranger)).await.unwrap_err();
        assert_eq!(unknown.code(), Code::PermissionDenied);
        let mut tampered = signed(0, "n-4", now, &glass_key);
        tampered.reason = "a different reason".into();
        assert_eq!(c.break_glass_rollback(tampered).await.unwrap_err().code(), Code::PermissionDenied);
    }
}

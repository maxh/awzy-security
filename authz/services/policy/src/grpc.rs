//! `awzy.policy.v1.PolicyService` over gRPC.

use awzy_service_auth::{require_by_path, Allowed, PathGuard, Rule};
use std::sync::Arc;

use awzy_authz_proto::{
    policy::v1::{
        get_bundle_request::Key,
        policy_service_server::{PolicyService, PolicyServiceServer},
        BreakGlassRollbackRequest, BreakGlassRollbackResponse, BundleStatus as WireStatus, CreateDraftRequest,
        CreateDraftResponse, EnforcementSetting as WireSetting, GetActiveRequest, GetActiveResponse, GetBundleRequest,
        GetBundleResponse, GetEnforcementRequest, GetEnforcementResponse, ListBundlesRequest, ListBundlesResponse,
        ListEnforcementChangesRequest, ListEnforcementChangesResponse, PolicyBundle as WireBundle, PublishRequest,
        PublishResponse, SetEnforcementRequest, SetEnforcementResponse,
    },
    prost_types::Timestamp,
};
use awzy_model::PolicyBundleId;
use axum::Router;
use chrono::{DateTime, Utc};
use tonic::{Request, Response, Status};

use awzy_signing::{break_glass_payload, BundleVerifier};

use crate::{
    BreakGlassRequest, BundleStatus, EnforcementSetting, PolicyBundle, PolicyStore, StoreError, INSTALL_SCOPE,
};

/// Who may call which methods (ADR 0055). Each group's callers come from the chart, as
/// `namespace/service-account`; a method no group names is refused.
pub struct Access {
    verifier: Arc<awzy_service_auth::Verifier>,
    /// Reading bundles and the mode: the PDP and the client portal.
    readers: Allowed,
    /// Drafting, publishing, and switching the mode: the client portal.
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
            ["GetActive", "GetEnforcement", "ListBundles", "GetBundle", "ListEnforcementChanges"]
                .into_iter()
                .map(|m| (method(m), Rule::Allow(self.readers.clone())))
                .collect();
        rules.extend(
            ["CreateDraft", "Publish", "SetEnforcement"]
                .into_iter()
                .map(|m| (method(m), Rule::Allow(self.writers.clone()))),
        );
        // Signed by enrolled admin keys, and needed when everything else is broken (ADR 0022).
        rules.push((method("BreakGlassRollback"), Rule::Exempt));
        PathGuard::new(self.verifier.clone(), rules)
    }
}

/// `break_glass`: the public keys enrolled at install time for break-glass rollback. None turns
/// the RPC off. `access`: who may call which methods.
pub fn router(store: Arc<PolicyStore>, break_glass: Option<Arc<BundleVerifier>>, access: &Access) -> Router {
    tonic::service::Routes::new(PolicyServiceServer::new(Service(store, break_glass)))
        .into_axum_router()
        .route_layer(axum::middleware::from_fn_with_state(access.guard(), require_by_path))
}

/// How far a break-glass request's timestamp may be from now.
const BREAK_GLASS_WINDOW_SECS: i64 = 300;

struct Service(Arc<PolicyStore>, Option<Arc<BundleVerifier>>);

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
        let known = req.into_inner().known_version;
        let res = match self.0.active().await.map_err(status)? {
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
        // TODO: notify every admin (Slack, Teams, email) once notifications exist.
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
        let app = router(Arc::new(store), Some(Arc::new(enrolled)), &Access::off());
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

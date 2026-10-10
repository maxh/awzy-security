//! Keeps the PDP on the active policy bundle and the install's enforcement mode from awzy-policy.
//!
//! Polls `GetActive` with the version in force; awzy-policy answers `unchanged` until a new
//! bundle is published. A bundle that fails to load is logged and the current one stays. The
//! enforcement mode is read on every poll; the first successful poll makes the PDP ready.

use awzy_service_auth::{grpc_channel, AuthChannel};
use std::{sync::Arc, time::Duration};

use awzy_authz_proto::policy::v1::{
    policy_service_client::PolicyServiceClient, GetActiveRequest, GetEnforcementRequest,
};
use awzy_model::EnforcementMode;
use tonic::transport::Channel;

use awzy_signing::BundleVerifier;

use crate::{Enforcement, Pdp, SharedPdp};

/// Runs forever, checking for a new active bundle and mode every `every`. With a verifier,
/// bundles that aren't signed by a trusted key are refused and the current one stays.
pub async fn sync_policies(
    pdp: SharedPdp,
    enforcement: Arc<Enforcement>,
    verifier: Option<Arc<BundleVerifier>>,
    endpoint: String,
    every: Duration,
) {
    let mut client = None;
    loop {
        if let Err(error) = sync_once(&pdp, &enforcement, verifier.as_deref(), &endpoint, &mut client).await {
            tracing::warn!(%error, %endpoint, "policy sync failed; keeping the current bundle");
            client = None;
        }
        tokio::time::sleep(every).await;
    }
}

/// One check. Returns true when a new bundle was loaded.
pub async fn sync_once(
    pdp: &SharedPdp,
    enforcement: &Enforcement,
    verifier: Option<&BundleVerifier>,
    endpoint: &str,
    client: &mut Option<PolicyServiceClient<AuthChannel>>,
) -> anyhow::Result<bool> {
    if client.is_none() {
        // With this PDP's service token for awzy-policy (ADR 0055).
        let channel = Channel::from_shared(endpoint.to_string())?.connect().await?;
        *client = Some(PolicyServiceClient::new(grpc_channel(channel, "awzy-policy")));
    }
    let client = client.as_mut().expect("connected above");

    let known_version = pdp.load().version();
    let res = client.get_active(GetActiveRequest { known_version }).await?.into_inner();
    let loaded = match res.bundle.filter(|_| !res.unchanged) {
        Some(bundle) => {
            if let Some(verifier) = verifier {
                verifier
                    .verify(
                        &bundle.qid,
                        bundle.version,
                        &bundle.schema,
                        &bundle.policies,
                        &bundle.signature,
                        &bundle.signing_key_id,
                    )
                    .map_err(|e| {
                        anyhow::anyhow!("refusing policy bundle {} (version {}): {e}", bundle.qid, bundle.version)
                    })?;
            }
            let next = Pdp::new(&bundle.schema, &bundle.policies)?.with_version(bundle.version);
            pdp.store(Arc::new(next));
            tracing::info!(version = bundle.version, id = %bundle.qid, "loaded policy bundle");
            true
        }
        None => false,
    };

    // Read the mode after the bundle, so a PDP is never ready with a mode but no bundle.
    let setting = client
        .get_enforcement(GetEnforcementRequest { scope: String::new() })
        .await?
        .into_inner()
        .setting
        .ok_or_else(|| anyhow::anyhow!("awzy-policy returned no enforcement setting"))?;
    let mode = EnforcementMode::from(setting.mode());
    if enforcement.synced(mode) {
        tracing::info!(mode = mode.as_str(), changed_by = %setting.changed_by, "enforcement mode");
    }
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use awzy_authz_proto::policy::v1::GetEnforcementRequest;
    use awzy_authz_proto::policy::v1::{
        policy_service_server::{PolicyService, PolicyServiceServer},
        CreateDraftRequest, CreateDraftResponse, EnforcementSetting, GetActiveResponse, GetBundleRequest,
        GetBundleResponse, GetEnforcementResponse, ListBundlesRequest, ListBundlesResponse,
        ListEnforcementChangesRequest, ListEnforcementChangesResponse, PolicyBundle, PublishRequest, PublishResponse,
        SetEnforcementRequest, SetEnforcementResponse,
    };
    use awzy_model::{AccessRequest, EntityRef, Verdict};
    use tonic::{Request, Response, Status};

    /// Serves one fixed active bundle at version 7.
    struct FakePolicy;

    #[tonic::async_trait]
    impl PolicyService for FakePolicy {
        async fn get_active(&self, req: Request<GetActiveRequest>) -> Result<Response<GetActiveResponse>, Status> {
            if req.into_inner().known_version == 7 {
                return Ok(Response::new(GetActiveResponse { bundle: None, unchanged: true }));
            }
            Ok(Response::new(GetActiveResponse {
                bundle: Some(PolicyBundle {
                    qid: awzy_model::PolicyBundleId::new().to_string(),
                    version: 7,
                    schema: crate::DEFAULT_SCHEMA.into(),
                    policies: r#"@id("everyone-reads") permit (principal, action == Action::"read", resource);"#.into(),
                    ..Default::default()
                }),
                unchanged: false,
            }))
        }
        async fn create_draft(&self, _: Request<CreateDraftRequest>) -> Result<Response<CreateDraftResponse>, Status> {
            Err(Status::unimplemented("fake"))
        }
        async fn publish(&self, _: Request<PublishRequest>) -> Result<Response<PublishResponse>, Status> {
            Err(Status::unimplemented("fake"))
        }
        async fn list_bundles(&self, _: Request<ListBundlesRequest>) -> Result<Response<ListBundlesResponse>, Status> {
            Err(Status::unimplemented("fake"))
        }
        async fn get_bundle(&self, _: Request<GetBundleRequest>) -> Result<Response<GetBundleResponse>, Status> {
            Err(Status::unimplemented("fake"))
        }
        async fn get_enforcement(
            &self,
            _: Request<GetEnforcementRequest>,
        ) -> Result<Response<GetEnforcementResponse>, Status> {
            let mode = awzy_authz_proto::authz::v1::EnforcementMode::Enforce.into();
            Ok(Response::new(GetEnforcementResponse {
                setting: Some(EnforcementSetting {
                    scope: "install".into(),
                    mode,
                    changed_by: "alice".into(),
                    changed_at: None,
                }),
            }))
        }
        async fn set_enforcement(
            &self,
            _: Request<SetEnforcementRequest>,
        ) -> Result<Response<SetEnforcementResponse>, Status> {
            Err(Status::unimplemented("fake"))
        }
        async fn break_glass_rollback(
            &self,
            _: Request<awzy_authz_proto::policy::v1::BreakGlassRollbackRequest>,
        ) -> Result<Response<awzy_authz_proto::policy::v1::BreakGlassRollbackResponse>, Status> {
            Err(Status::unimplemented("fake"))
        }
        async fn list_enforcement_changes(
            &self,
            _: Request<ListEnforcementChangesRequest>,
        ) -> Result<Response<ListEnforcementChangesResponse>, Status> {
            Err(Status::unimplemented("fake"))
        }
    }

    #[tokio::test]
    async fn loads_new_bundle_then_reports_unchanged() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = tonic::service::Routes::new(PolicyServiceServer::new(FakePolicy)).into_axum_router();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let pdp = crate::shared(Pdp::with_defaults().unwrap());
        let enforcement = Enforcement::awaiting_sync();
        let endpoint = format!("http://{addr}");
        let mut client = None;

        // A PDP that verifies refuses the fake's unsigned bundle, and isn't ready.
        let key = p256::ecdsa::SigningKey::from_bytes(&[5u8; 32].into()).unwrap();
        let verifier = BundleVerifier::from_keys([*key.verifying_key()]).unwrap();
        let err = sync_once(&pdp, &enforcement, Some(&verifier), &endpoint, &mut client).await.unwrap_err();
        assert!(err.to_string().contains("not signed"), "{err}");
        assert!(!enforcement.is_ready());
        assert_eq!(pdp.load().version(), 0);

        assert!(sync_once(&pdp, &enforcement, None, &endpoint, &mut client).await.unwrap());
        assert_eq!(pdp.load().version(), 7);
        assert!(enforcement.is_ready());
        assert_eq!(enforcement.mode(), EnforcementMode::Enforce);
        assert!(!sync_once(&pdp, &enforcement, None, &endpoint, &mut client).await.unwrap(), "same version is a no-op");

        // The default policies would deny bob reading crown jewels; bundle 7 allows it.
        let entities = pdp
            .load()
            .entities(serde_json::json!([
                { "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [] },
                { "uid": { "type": "Resource", "id": "board-deck" }, "attrs": { "sensitivity": "crown_jewels" }, "parents": [] },
            ]))
            .unwrap();
        let req = AccessRequest {
            subject: EntityRef::new("Human", "bob"),
            action: "read".into(),
            resource: EntityRef::new("Resource", "board-deck"),
            context: serde_json::json!({ "risk_score": 1 }).as_object().unwrap().clone(),
            on_behalf_of: None,
            human_present: None,
            always_enforced: false,
        };
        let decision = pdp.load().decide(&req, &entities).unwrap();
        assert_eq!(decision.verdict, Verdict::Allow);
        assert_eq!(decision.policies, vec!["everyone-reads"]);
    }
}

//! Keeps the PDP on the active policy bundle and the install's enforcement mode from awzy-policy.
//!
//! Polls `GetActive` with the version in force; awzy-policy answers `unchanged` until a new
//! bundle is published. A bundle that fails to load is logged and the current one stays. The
//! enforcement mode is read on every poll; the first successful poll makes the PDP ready.

use awzy_service_auth::{grpc_channel, AuthChannel};
use std::{sync::Arc, time::Duration};

use awzy_authz_proto::policy::v1::{
    policy_service_client::PolicyServiceClient, EnforcementSetting, GetActiveRequest, GetEnforcementRequest,
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
        // Wait in awzy-policy for a new bundle or mode once there is one to compare (ADR 0077).
        let wait = if pdp.load().version() > 0 { WAIT_SECS } else { 0 };
        match sync_waiting(&pdp, &enforcement, verifier.as_deref(), &endpoint, &mut client, wait).await {
            Ok(_) => {
                crate::metrics::POLICY_SYNC.succeeded();
                if wait > 0 {
                    continue;
                }
            }
            Err(error) => {
                crate::metrics::POLICY_SYNC.failed();
                tracing::warn!(%error, %endpoint, "policy sync failed; keeping the current bundle");
                client = None;
            }
        }
        tokio::time::sleep(every).await;
    }
}

/// How long awzy-policy holds a request when nothing has changed.
const WAIT_SECS: u32 = 25;

/// One check, answered at once. Returns true when a new bundle was loaded.
pub async fn sync_once(
    pdp: &SharedPdp,
    enforcement: &Enforcement,
    verifier: Option<&BundleVerifier>,
    endpoint: &str,
    client: &mut Option<PolicyServiceClient<AuthChannel>>,
) -> anyhow::Result<bool> {
    sync_waiting(pdp, enforcement, verifier, endpoint, client, 0).await
}

/// One check, which awzy-policy holds up to `wait_seconds` for a new bundle or mode.
pub async fn sync_waiting(
    pdp: &SharedPdp,
    enforcement: &Enforcement,
    verifier: Option<&BundleVerifier>,
    endpoint: &str,
    client: &mut Option<PolicyServiceClient<AuthChannel>>,
    wait_seconds: u32,
) -> anyhow::Result<bool> {
    if client.is_none() {
        // With this PDP's service token for awzy-policy (ADR 0055).
        let channel = Channel::from_shared(endpoint.to_string())?.connect().await?;
        *client = Some(PolicyServiceClient::new(grpc_channel(channel, "awzy-policy")));
    }
    let client = client.as_mut().expect("connected above");

    let known_version = pdp.load().version();
    let res = client.get_active(GetActiveRequest { known_version, wait_seconds }).await?.into_inner();
    let loaded = match res.bundle.filter(|_| !res.unchanged) {
        Some(bundle) => {
            // Versions only go up, break-glass included (it republishes as a new version), so an
            // older one is a signed bundle replayed from the database (ADR 0080).
            anyhow::ensure!(
                bundle.version > known_version,
                "refusing policy bundle {} (version {}): older than version {known_version}, in force",
                bundle.qid,
                bundle.version
            );
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
    if let Some(verifier) = verifier {
        verify_setting(verifier, enforcement, &setting, mode)?;
    }
    if enforcement.synced(mode) {
        tracing::info!(mode = mode.as_str(), changed_by = %setting.changed_by, "enforcement mode");
    }
    Ok(loaded)
}

/// Applies a mode only if awzy-policy signed it and it's no older than the one in force (ADR
/// 0080). A scope never switched comes unsigned, and is monitor.
fn verify_setting(
    verifier: &BundleVerifier,
    enforcement: &Enforcement,
    setting: &EnforcementSetting,
    mode: EnforcementMode,
) -> anyhow::Result<()> {
    let Some(at) = setting.changed_at.as_ref().map(|t| t.seconds) else {
        anyhow::ensure!(
            mode == EnforcementMode::Monitor && enforcement.changed_at().is_none(),
            "refusing an enforcement mode with no change behind it"
        );
        return Ok(());
    };
    let payload = awzy_signing::enforcement_payload(&setting.scope, mode.as_str(), &setting.changed_by, at);
    verifier
        .verify_payload(&payload, &setting.signature, &setting.signing_key_id)
        .map_err(|e| anyhow::anyhow!("refusing the enforcement mode set at {at}: {e}"))?;
    if let Some(applied) = enforcement.changed_at() {
        anyhow::ensure!(
            at >= applied,
            "refusing an enforcement mode set at {at}, older than the one in force ({applied})"
        );
    }
    enforcement.set_changed_at(at);
    Ok(())
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
    use awzy_model::{Check, EntityRef, Verdict};
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
                    ..Default::default()
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
        let req = Check {
            principal: EntityRef::new("Human", "bob"),
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

    #[test]
    fn only_a_signed_mode_no_older_than_the_one_in_force_applies() {
        let key = p256::ecdsa::SigningKey::from_bytes(&[5u8; 32].into()).unwrap();
        let signer = awzy_signing::BundleSigner::from_key(key.clone()).unwrap();
        let verifier = BundleVerifier::from_keys([*key.verifying_key()]).unwrap();
        let setting = |mode: EnforcementMode, at: i64| EnforcementSetting {
            scope: "install".into(),
            mode: awzy_authz_proto::authz::v1::EnforcementMode::from(mode).into(),
            changed_by: "Human::alice".into(),
            changed_at: Some(awzy_authz_proto::prost_types::Timestamp { seconds: at, nanos: 0 }),
            signature: signer.sign_payload(&awzy_signing::enforcement_payload(
                "install",
                mode.as_str(),
                "Human::alice",
                at,
            )),
            signing_key_id: signer.key_id().into(),
            ..Default::default()
        };
        let enforcement = Enforcement::awaiting_sync();
        let (monitor, enforce) = (EnforcementMode::Monitor, EnforcementMode::Enforce);

        assert!(
            verify_setting(&verifier, &enforcement, &EnforcementSetting::default(), monitor).is_ok(),
            "never switched"
        );
        assert!(verify_setting(&verifier, &enforcement, &setting(enforce, 2_000), enforce).is_ok());

        let mut flipped = setting(enforce, 3_000);
        flipped.mode = awzy_authz_proto::authz::v1::EnforcementMode::Monitor.into();
        assert!(verify_setting(&verifier, &enforcement, &flipped, monitor).is_err(), "changed in the database");
        assert!(verify_setting(&verifier, &enforcement, &setting(monitor, 1_000), monitor).is_err(), "an older change");
        assert!(
            verify_setting(&verifier, &enforcement, &EnforcementSetting::default(), monitor).is_err(),
            "every change deleted"
        );
        assert!(verify_setting(&verifier, &enforcement, &setting(monitor, 3_000), monitor).is_ok(), "a newer one");
    }
}

//! What a running PDP passes one that's starting (ADR 0119).
//!
//! A PDP is ready only once it has the policy bundle and mode, the principals snapshot, and the
//! containment list. If awzy-policy, awzy-context, or awzy-challenge can't be reached when it
//! starts, it asks the PDPs already running for what it's missing, through a headless Service
//! that lists only ready pods. Policy is verified as if awzy-policy had sent it; the snapshot and
//! the containment list are trusted as their sources are, since only the install's backend may
//! call the peer service (ADR 0055). Each input keeps its age, so it doesn't look fresher for the
//! hop, and whatever a source sends later replaces it.

use std::time::Duration;

use awzy_authz_proto::pdp::v1::{
    pdp_peer_service_client::PdpPeerServiceClient,
    pdp_peer_service_server::{PdpPeerService, PdpPeerServiceServer},
    ContainmentsState, GetResourceRequest, GetResourceResponse, GetStateRequest, GetStateResponse, PolicyState,
    PrincipalsState,
};
use awzy_service_auth::{grpc_channel, Allowed};
use awzy_signing::BundleVerifier;
use axum::Router;
use tonic::{server::NamedService, transport::Channel, Request, Response, Status};

use crate::AppState;

/// How long a starting PDP gives its sources before asking peers, and how long between tries.
const RETRY: Duration = Duration::from_secs(5);
/// The most peers asked each try, and how long each may take.
const MOST_PEERS: usize = 5;
const PEER_TIMEOUT: Duration = Duration::from_secs(5);
/// A principals snapshot grows with headcount, past gRPC's 4 MB default.
const MAX_STATE_BYTES: usize = 256 << 20;
/// How long a check waits on each peer for a resource: it's already past awzy-context's timeout.
const RESOURCE_TIMEOUT: Duration = Duration::from_millis(250);

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

struct Service(AppState);

#[tonic::async_trait]
impl PdpPeerService for Service {
    async fn get_state(&self, _: Request<GetStateRequest>) -> Result<Response<GetStateResponse>, Status> {
        state_of(&self.0).map(Response::new).map_err(|e| Status::internal(e.to_string()))
    }

    async fn get_resource(
        &self,
        request: Request<GetResourceRequest>,
    ) -> Result<Response<GetResourceResponse>, Status> {
        let id = request.into_inner().resource_id;
        let res = match self.0.resources.held(&id) {
            Some((known, entities_json, age)) => {
                GetResourceResponse { cached: true, known, entities_json, age_ms: millis(age) }
            }
            None => GetResourceResponse::default(),
        };
        Ok(Response::new(res))
    }
}

/// The peer service, which only the callers in `AWZY_PDP_PEERS` may call: the install's backend.
/// A route of its own rather than tonic's `Routes`, whose fallback can't be merged twice.
pub fn router(state: AppState) -> Router {
    let guard = awzy_service_auth::Guard::new(state.service_auth.clone(), Allowed::from_env("AWZY_PDP_PEERS"));
    let service = PdpPeerServiceServer::new(Service(state)).max_encoding_message_size(MAX_STATE_BYTES);
    let path = format!("/{}/{{*rest}}", <PdpPeerServiceServer<Service> as NamedService>::NAME);
    Router::new()
        .route_service(&path, service)
        .route_layer(axum::middleware::from_fn_with_state(guard, awzy_service_auth::require))
}

/// What this PDP can pass on: each input it has loaded from its source, or from a peer.
pub fn state_of(state: &AppState) -> anyhow::Result<GetStateResponse> {
    let signed = state.enforcement.signed();
    let policy = match signed.install {
        Some(install) if state.enforcement.is_ready() => Some(PolicyState {
            // Unset while awzy-policy has no bundle published: the built-in defaults.
            bundle: state.pdp.load().signed().map(|b| b.as_ref().clone()),
            install: Some(install),
            scoped: signed.scoped.into_values().collect(),
            age_ms: millis(crate::metrics::POLICY_SYNC.age()),
        }),
        _ => None,
    };
    let principals = match state.principals.held() {
        Some(body) => Some(PrincipalsState {
            version: body.version,
            snapshot_json: serde_json::to_vec(&body)?,
            age_ms: millis(crate::metrics::PRINCIPALS_SYNC.age()),
        }),
        None => None,
    };
    let containments = state.containments.held().map(|(version, active, age)| ContainmentsState {
        version,
        active,
        age_ms: millis(age),
    });
    Ok(GetStateResponse { policy, principals, containments })
}

/// Until this PDP is ready, asks the PDPs behind `peers` (a headless Service, as `host:port`) for
/// whatever its sources haven't sent.
pub async fn start_from_peers(state: AppState, verifier: Option<std::sync::Arc<BundleVerifier>>, peers: String) {
    loop {
        tokio::time::sleep(RETRY).await;
        if state.is_ready() {
            return;
        }
        let answers = ask_peers(&peers).await;
        if answers.is_empty() {
            tracing::warn!(%peers, "not ready, and no running PDP to start from");
            continue;
        }
        let loaded = apply(&state, verifier.as_deref(), answers);
        if !loaded.is_empty() {
            tracing::warn!(?loaded, "started from a peer's state; the sources haven't answered yet");
        }
    }
}

/// The state of up to `MOST_PEERS` of the PDPs behind `peers`. The ones that don't answer are
/// logged and left out.
async fn ask_peers(peers: &str) -> Vec<GetStateResponse> {
    let addrs: Vec<_> = match tokio::net::lookup_host(peers).await {
        Ok(addrs) => addrs.take(MOST_PEERS).collect(),
        Err(error) => {
            tracing::warn!(%error, %peers, "couldn't look up the PDP's peers");
            return vec![];
        }
    };
    let mut answers = Vec::new();
    for addr in addrs {
        let ask = async {
            let channel = Channel::from_shared(format!("http://{addr}"))?.connect().await?;
            let mut client =
                PdpPeerServiceClient::new(grpc_channel(channel, "awzy-pdp")).max_decoding_message_size(MAX_STATE_BYTES);
            anyhow::Ok(client.get_state(GetStateRequest {}).await?.into_inner())
        };
        match tokio::time::timeout(PEER_TIMEOUT, ask).await {
            Ok(Ok(answer)) => answers.push(answer),
            Ok(Err(error)) => tracing::warn!(%error, %addr, "a peer didn't pass its state"),
            Err(_) => tracing::warn!(%addr, "a peer didn't pass its state in time"),
        }
    }
    answers
}

/// What the first of the PDPs behind `peers` that has resource `id` cached says about it:
/// whether awzy-context knew it, its entities as JSON, and its age.
pub(crate) async fn ask_for_resource(peers: &str, id: &str) -> Option<(bool, Vec<u8>, Duration)> {
    let addrs: Vec<_> = tokio::net::lookup_host(peers).await.ok()?.take(MOST_PEERS).collect();
    for addr in addrs {
        let ask = async {
            let channel = Channel::from_shared(format!("http://{addr}"))?.connect().await?;
            let mut client = PdpPeerServiceClient::new(grpc_channel(channel, "awzy-pdp"));
            let req = GetResourceRequest { resource_id: id.to_string() };
            anyhow::Ok(client.get_resource(req).await?.into_inner())
        };
        match tokio::time::timeout(RESOURCE_TIMEOUT, ask).await {
            Ok(Ok(res)) if res.cached => {
                return Some((res.known, res.entities_json, Duration::from_millis(res.age_ms)));
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) => tracing::debug!(%error, %addr, "a peer couldn't be asked for a resource"),
            Err(_) => tracing::debug!(%addr, "a peer didn't answer for a resource in time"),
        }
    }
    None
}

/// Loads the newest of each input this PDP is still missing from the peers' answers. Returns
/// the inputs it loaded.
pub fn apply(state: &AppState, verifier: Option<&BundleVerifier>, answers: Vec<GetStateResponse>) -> Vec<&'static str> {
    let mut policies = Vec::new();
    let mut principals = Vec::new();
    let mut containments = Vec::new();
    for answer in answers {
        policies.extend(answer.policy);
        principals.extend(answer.principals);
        containments.extend(answer.containments);
    }
    let mut loaded = Vec::new();
    if !state.enforcement.is_ready() {
        let newest = policies.into_iter().max_by_key(|p| p.bundle.as_ref().map_or(0, |b| b.version));
        if let Some(policy) = newest {
            match apply_policy(state, verifier, policy) {
                Ok(()) => loaded.push("policy"),
                Err(error) => tracing::error!(%error, "refused a peer's policy"),
            }
        }
    }
    if !state.principals.is_ready() {
        if let Some(newest) = principals.into_iter().max_by_key(|p| p.version) {
            let applied = serde_json::from_slice(&newest.snapshot_json)
                .map_err(anyhow::Error::from)
                .and_then(|body| state.principals.apply(body));
            match applied {
                Ok(_) => {
                    crate::metrics::PRINCIPALS_SYNC.succeeded_ago(Duration::from_millis(newest.age_ms));
                    loaded.push("principals");
                }
                Err(error) => tracing::error!(%error, "couldn't load a peer's principals snapshot"),
            }
        }
    }
    if state.containments.is_configured() && !state.containments.is_ready() {
        if let Some(newest) = containments.into_iter().max_by_key(|c| c.version) {
            if state.containments.load_list(newest.version, newest.active, Duration::from_millis(newest.age_ms)) {
                loaded.push("containments");
            }
        }
    }
    loaded
}

/// A peer's bundle and modes, checked exactly as awzy-policy's are.
fn apply_policy(state: &AppState, verifier: Option<&BundleVerifier>, policy: PolicyState) -> anyhow::Result<()> {
    if let Some(bundle) = policy.bundle {
        crate::sync::apply_bundle(&state.pdp, verifier, bundle)?;
    }
    let install = policy.install.ok_or_else(|| anyhow::anyhow!("the peer sent no enforcement mode"))?;
    crate::sync::apply_setting(&state.enforcement, verifier, install)?;
    crate::sync::apply_scoped(&state.enforcement, verifier, policy.scoped);
    crate::metrics::POLICY_SYNC.succeeded_ago(Duration::from_millis(policy.age_ms));
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use awzy_authz_proto::{
        challenge::v1::Containment,
        policy::v1::{EnforcementSetting, PolicyBundle},
    };
    use awzy_model::{EnforcementMode, EntityRef};
    use serde_json::json;

    use super::*;
    use crate::{containments::Containments, principals::Principals, Enforcement};

    const POLICIES: &str = r#"@id("everyone-reads") permit (principal, action == Action::"read", resource);"#;

    fn signer() -> (awzy_signing::BundleSigner, BundleVerifier) {
        let key = p256::ecdsa::SigningKey::from_bytes(&[7u8; 32].into()).unwrap();
        let verifier = BundleVerifier::from_keys([*key.verifying_key()]).unwrap();
        (awzy_signing::BundleSigner::from_key(key).unwrap(), verifier)
    }

    /// A PDP as it starts: waiting on awzy-policy, awzy-context, and awzy-challenge.
    fn starting() -> AppState {
        AppState::new(crate::shared(crate::Pdp::with_defaults().unwrap()))
            .with_enforcement(Arc::new(Enforcement::awaiting_sync()))
            .with_principals(Arc::new(Principals::awaiting_sync()))
            .with_challenges(
                Arc::new(Containments::from_challenge("http://127.0.0.1:9").unwrap()),
                Arc::new(crate::challenges::ChallengeResolver::none()),
            )
    }

    /// A running PDP that loaded a signed bundle and mode, a snapshot, and a containment list.
    fn running(signer: &awzy_signing::BundleSigner, verifier: &BundleVerifier) -> AppState {
        let state = starting();
        let bundle_qid = awzy_model::PolicyBundleId::new().to_string();
        let bundle = PolicyBundle {
            signature: signer.sign(&bundle_qid, 3, crate::DEFAULT_SCHEMA, POLICIES),
            signing_key_id: signer.key_id().into(),
            qid: bundle_qid,
            version: 3,
            schema: crate::DEFAULT_SCHEMA.into(),
            policies: POLICIES.into(),
            ..Default::default()
        };
        crate::sync::apply_bundle(&state.pdp, Some(verifier), bundle).unwrap();
        let at = 1_700_000_000;
        let install = EnforcementSetting {
            scope: "install".into(),
            mode: awzy_authz_proto::authz::v1::EnforcementMode::Enforce.into(),
            changed_by: "Human::alice".into(),
            changed_at: Some(awzy_authz_proto::prost_types::Timestamp { seconds: at, nanos: 0 }),
            signature: signer.sign_payload(&awzy_signing::enforcement_payload(
                "install",
                "enforce",
                "Human::alice",
                at,
            )),
            signing_key_id: signer.key_id().into(),
            ..Default::default()
        };
        crate::sync::apply_setting(&state.enforcement, Some(verifier), install).unwrap();
        state
            .principals
            .apply(
                serde_json::from_value(json!({
                    "version": 12,
                    "entities": [{ "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [] }],
                }))
                .unwrap(),
            )
            .unwrap();
        let mallory = Containment {
            qid: "containment_1".into(),
            principal: Some(EntityRef::new("Human", "mallory").into()),
            ..Default::default()
        };
        state.containments.load_list(4, vec![mallory], Duration::ZERO);
        state
    }

    #[tokio::test]
    async fn a_starting_pdp_is_ready_on_a_running_ones_state() {
        let (signer, verifier) = signer();
        let peer = running(&signer, &verifier);
        let state = starting();
        assert!(!state.is_ready());

        let loaded = apply(&state, Some(&verifier), vec![state_of(&peer).unwrap()]);
        assert_eq!(loaded, ["policy", "principals", "containments"]);
        assert!(state.is_ready());
        assert_eq!(state.pdp.load().version(), 3);
        assert_eq!(state.enforcement.mode(), EnforcementMode::Enforce);
        assert_eq!(state.principals.version(), 12);
        let found = state.containments.find(&[&EntityRef::new("Human", "mallory")]).await.unwrap();
        assert_eq!(found, ["containment_1"], "the peer's list answers");
    }

    #[tokio::test]
    async fn a_peers_policy_must_verify() {
        let (signer, verifier) = signer();
        let mut answer = state_of(&running(&signer, &verifier)).unwrap();
        let policy = answer.policy.as_mut().unwrap();
        policy.bundle.as_mut().unwrap().policies = r#"permit (principal, action, resource);"#.into();

        let state = starting();
        let loaded = apply(&state, Some(&verifier), vec![answer]);
        assert!(!loaded.contains(&"policy"), "an altered bundle is refused");
        assert!(!state.enforcement.is_ready(), "and the PDP stays not ready");
        assert_eq!(state.pdp.load().version(), 0);
    }

    #[tokio::test]
    async fn an_input_the_pdp_already_has_is_left_alone() {
        let (signer, verifier) = signer();
        let answer = state_of(&running(&signer, &verifier)).unwrap();
        let state = starting();
        state.principals.apply(serde_json::from_value(json!({ "version": 20, "entities": [] })).unwrap()).unwrap();

        let loaded = apply(&state, Some(&verifier), vec![answer]);
        assert_eq!(loaded, ["policy", "containments"]);
        assert_eq!(state.principals.version(), 20, "awzy-context's newer snapshot stays");
    }

    #[tokio::test]
    async fn the_state_crosses_the_wire() {
        let (signer, verifier) = signer();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = crate::router(running(&signer, &verifier));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let answers = ask_peers(&addr.to_string()).await;
        assert_eq!(answers.len(), 1);
        let state = starting();
        assert_eq!(apply(&state, Some(&verifier), answers).len(), 3);
        assert!(state.is_ready());
    }
}

use std::sync::Arc;

use anyhow::Context;
use awzy_authz_proto::{
    authz::v1::{authorizer_client::AuthorizerClient, ListTrustedIssuersRequest},
    challenge::v1::challenge_service_client::ChallengeServiceClient,
    policy::v1::policy_service_client::PolicyServiceClient,
};
use awzy_gateway::{
    containments::{self, Containments},
    fallback::{self, Fallback},
};
use tonic::transport::Channel;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-gateway");
    let path = std::env::var("AWZY_GATEWAY_CONFIG").context("AWZY_GATEWAY_CONFIG is not set")?;
    let config: awzy_gateway::GatewayConfig =
        serde_json::from_slice(&std::fs::read(&path).with_context(|| format!("reading {path}"))?)
            .with_context(|| format!("parsing {path}"))?;
    let pdp_addr = std::env::var("AWZY_PDP_ADDR").context("AWZY_PDP_ADDR is not set")?;
    // With this gateway's service token for the PDP (ADR 0055). A check takes milliseconds; the
    // timeouts bound how long a tool call waits on an unreachable PDP before the outage rules
    // apply (ADR 0030). Without them, each call in the example install's outage drill waited
    // about 8 seconds.
    // Several connections, taken in turn, so checks spread over PDP pods and don't queue on one
    // connection's lock (AWZY_GATEWAY_PDP_CONNECTIONS, default 4).
    let connections: usize =
        std::env::var("AWZY_GATEWAY_PDP_CONNECTIONS").ok().and_then(|n| n.parse().ok()).unwrap_or(4).max(1);
    let mut pdp = Vec::with_capacity(connections);
    for _ in 0..connections {
        pdp.push(AuthorizerClient::new(awzy_service_auth::grpc_channel(
            Channel::from_shared(pdp_addr.clone())?
                .connect_timeout(std::time::Duration::from_millis(500))
                .timeout(std::time::Duration::from_secs(2))
                .connect_lazy(),
            "awzy-pdp",
        )));
    }
    let issuers_pdp = pdp[0].clone();
    // Containments from awzy-challenge, so a PDP outage never replays an allow for a contained
    // principal (ADR 0030). Without it, outages replay as before.
    // Forwarded tool calls, recorded in awzy-intake (ADR 0084).
    let activity = awzy_activity::ActivityLog::from_env("awzy-gateway");
    // Tool tiers and fail-open tiers, set in the client portal and signed by awzy-policy (ADR
    // 0089). Without awzy-policy every tool is unclassified and outages fail closed.
    let fallback = Arc::new(Fallback::awaiting());
    let state = match std::env::var("AWZY_CHALLENGE_ADDR") {
        Ok(addr) => {
            let containments = Arc::new(Containments::awaiting_feed());
            let state = awzy_gateway::GatewayState::with_containments(config, pdp, containments.clone())?
                .with_activity(activity)
                .with_fallback(fallback.clone());
            let challenge = ChallengeServiceClient::new(awzy_service_auth::grpc_channel(
                Channel::from_shared(addr)?.connect_lazy(),
                "awzy-challenge",
            ));
            let forgetting = state.clone();
            tokio::spawn(containments::follow(containments, challenge, move |set| forgetting.forget_allows(set)));
            state
        }
        Err(_) => {
            tracing::warn!("AWZY_CHALLENGE_ADDR is not set; a PDP outage replays allows without checking containments");
            awzy_gateway::GatewayState::new(config, pdp)?.with_activity(activity).with_fallback(fallback.clone())
        }
    };
    // Apps trusted to sign tokens for their own agents (ADR 0110), from the PDP.
    tokio::spawn(awzy_identity::TrustedIssuers::follow(vec![state.trusted_issuers()], move || {
        let mut pdp = issuers_pdp.clone();
        async move {
            let res = pdp.list_trusted_issuers(ListTrustedIssuersRequest {}).await?;
            Ok(res.into_inner().issuers.into_iter().map(Into::into).collect())
        }
    }));
    match std::env::var("AWZY_POLICY_ADDR") {
        Ok(addr) => {
            let policy = PolicyServiceClient::new(awzy_service_auth::grpc_channel(
                Channel::from_shared(addr)?.connect_lazy(),
                "awzy-policy",
            ));
            // The key PDPs verify bundles with; without it, as when run locally, unsigned settings are taken.
            let verifier = match std::env::var("AWZY_POLICY_VERIFY_KEY_FILE") {
                Ok(path) => {
                    let pem = std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?;
                    Some(Arc::new(awzy_signing::BundleVerifier::from_pem_bundle(&pem)?))
                }
                Err(_) => {
                    tracing::warn!("AWZY_POLICY_VERIFY_KEY_FILE is not set; outage fallback settings aren't verified");
                    None
                }
            };
            tokio::spawn(fallback::follow(fallback, policy, verifier));
        }
        Err(_) => tracing::warn!("AWZY_POLICY_ADDR is not set; every tool is unclassified and outages fail closed"),
    }
    awzy_server::metrics::register(std::sync::Arc::new(awzy_gateway::metrics::GatewayMetrics));
    awzy_server::serve("awzy-gateway", awzy_gateway::router(state)).await
}

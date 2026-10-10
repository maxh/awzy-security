use std::sync::Arc;

use anyhow::Context;
use awzy_authz_proto::{
    authz::v1::authorizer_client::AuthorizerClient, challenge::v1::challenge_service_client::ChallengeServiceClient,
};
use awzy_gateway::containments::{self, Containments};
use tonic::transport::Channel;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-gateway");
    let path = std::env::var("AWZY_GATEWAY_CONFIG").context("AWZY_GATEWAY_CONFIG is not set")?;
    let config: awzy_gateway::GatewayConfig =
        serde_json::from_slice(&std::fs::read(&path).with_context(|| format!("reading {path}"))?)
            .with_context(|| format!("parsing {path}"))?;
    let pdp_addr = std::env::var("AWZY_PDP_ADDR").context("AWZY_PDP_ADDR is not set")?;
    // With this gateway's service token for the PDP (ADR 0055).
    let pdp = AuthorizerClient::new(awzy_service_auth::grpc_channel(
        Channel::from_shared(pdp_addr)?.connect_lazy(),
        "awzy-pdp",
    ));
    // Containments from awzy-challenge, so a PDP outage never replays an allow for a contained
    // accessor (ADR 0030). Without it, outages replay as before.
    let state = match std::env::var("AWZY_CHALLENGE_ADDR") {
        Ok(addr) => {
            let containments = Arc::new(Containments::awaiting_feed());
            let state = awzy_gateway::GatewayState::with_containments(config, pdp, containments.clone())?;
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
            awzy_gateway::GatewayState::new(config, pdp)?
        }
    };
    awzy_server::metrics::register(std::sync::Arc::new(awzy_gateway::metrics::GatewayMetrics));
    awzy_server::serve("awzy-gateway", awzy_gateway::router(state)).await
}

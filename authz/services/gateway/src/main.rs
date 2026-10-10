use anyhow::Context;
use awzy_authz_proto::authz::v1::authorizer_client::AuthorizerClient;
use tonic::transport::Channel;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-gateway");
    let path = std::env::var("AWZY_GATEWAY_CONFIG").context("AWZY_GATEWAY_CONFIG is not set")?;
    let config: awzy_gateway::GatewayConfig =
        serde_json::from_slice(&std::fs::read(&path).with_context(|| format!("reading {path}"))?)
            .with_context(|| format!("parsing {path}"))?;
    let pdp_addr = std::env::var("AWZY_PDP_ADDR").context("AWZY_PDP_ADDR is not set")?;
    // With this gateway's service token for the PDP (ADR 0054).
    let pdp = AuthorizerClient::new(awzy_service_auth::grpc_channel(
        Channel::from_shared(pdp_addr)?.connect_lazy(),
        "awzy-pdp",
    ));
    let state = awzy_gateway::GatewayState::new(config, pdp)?;
    awzy_server::serve("awzy-gateway", awzy_gateway::router(state)).await
}

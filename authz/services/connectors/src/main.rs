#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-connectors");
    awzy_server::serve("awzy-connectors", awzy_connectors::router()).await
}

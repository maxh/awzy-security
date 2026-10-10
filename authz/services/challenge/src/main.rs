#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-challenge");
    awzy_server::serve("awzy-challenge", awzy_challenge::router()).await
}

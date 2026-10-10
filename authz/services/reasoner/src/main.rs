#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-reasoner");
    awzy_server::serve("awzy-reasoner", awzy_reasoner::router()).await
}

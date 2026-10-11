use std::sync::Arc;

use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-intake");
    let token = std::env::var("AWZY_INTAKE_TOKEN").context("AWZY_INTAKE_TOKEN is not set")?;
    anyhow::ensure!(token.len() >= 32, "AWZY_INTAKE_TOKEN must be at least 32 characters");
    let brokers = awzy_kafka::Brokers::from_env("AWZY_KAFKA_BROKERS")?.context("AWZY_KAFKA_BROKERS is not set")?;
    let topic = std::env::var("AWZY_EVENTS_TOPIC").unwrap_or_else(|_| awzy_intake::DEFAULT_EVENTS_TOPIC.into());
    let log = awzy_intake::EventLog::connect_kafka(&brokers, &topic).await?;
    awzy_server::serve("awzy-intake", awzy_intake::router(Arc::new(log), &token)).await
}

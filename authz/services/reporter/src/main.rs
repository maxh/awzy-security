use axum::Router;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-reporter");
    if std::env::var("AWZY_REPORTER_ENABLED").as_deref() == Ok("false") {
        tracing::info!("reporting to awzy is turned off (AWZY_REPORTER_ENABLED=false); nothing is sent");
        return awzy_server::serve("awzy-reporter", Router::new()).await;
    }
    let config = awzy_reporter::Config::from_env()?;
    let mut reporter = awzy_reporter::Reporter::new(config.clone())?;
    // Support session requests go to awzy-challenge, with this reporter's service token (ADR 0095).
    if let Ok(addr) = std::env::var("AWZY_CHALLENGE_ADDR") {
        let channel = tonic::transport::Channel::from_shared(addr)?.connect_lazy();
        reporter = reporter.with_support(awzy_reporter::ChallengeClient::new(awzy_service_auth::grpc_channel(
            channel,
            "awzy-challenge",
        )));
    }
    // --print: show one heartbeat without sending it, so the client can see what would leave.
    if std::env::args().any(|a| a == "--print") {
        let hb = reporter.collect().await;
        println!("{}", serde_json::to_string_pretty(&hb)?);
        return Ok(());
    }
    tracing::info!(install = %config.install_id, corp = %config.corp_url, source = ?config.source, every = ?config.interval, "reporting to awzy");
    tokio::spawn(awzy_reporter::run(reporter));
    awzy_server::serve("awzy-reporter", Router::new()).await
}

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
    // --print: show one heartbeat without sending it, so the client can see what would leave.
    if std::env::args().any(|a| a == "--print") {
        let hb = reporter.collect().await;
        println!("{}", serde_json::to_string_pretty(&hb)?);
        return Ok(());
    }
    tracing::info!(install = %config.install_id, corp = %config.corp_url, source = ?config.source, every = ?config.interval, "reporting to awzy");
    let last = awzy_reporter::LastSent::default();
    let app = awzy_reporter::router(&config, last.clone());
    tokio::spawn(awzy_reporter::run(reporter, last));
    awzy_server::serve("awzy-reporter", app).await
}

use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-corp-portal");
    let url = std::env::var("AWZY_POSTGRES_URL").context("AWZY_POSTGRES_URL is not set")?;
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await?;
    // The registry proxy's pull tokens live in its own schema of this database (ADR 0028).
    let pull_tokens = awzy_registry::tokens::TokenStore::connect(&url, "registry").await?;
    let store = awzy_corp_portal::fleet::FleetStore::new(pool).with_pull_tokens(pull_tokens);
    // TODO: run migrations from a release Job instead of at startup.
    store.migrate().await?;
    // Fleet alerts to Slack (ADR 0043).
    let slack = std::env::var("AWZY_CORP_PORTAL_ALERTS_SLACK_WEBHOOK_URL")
        .ok()
        .and_then(|url| awzy_corp_portal::alerts::SlackWebhook::from_url(url.trim()));
    let corp_url = std::env::var("AWZY_CORP_PORTAL_PUBLIC_URL").ok().map(|u| u.trim_end_matches('/').to_string());
    tokio::spawn(awzy_corp_portal::alerts::run(store.clone(), slack, corp_url));
    let mut app = awzy_corp_portal::router(awzy_corp_portal::auth::Auth::from_env()?, store);
    if let Some(dir) = awzy_portal::frontend_dir_from_env() {
        app = awzy_portal::with_frontend(app, &dir);
    }
    awzy_server::serve("awzy-corp-portal", app).await
}

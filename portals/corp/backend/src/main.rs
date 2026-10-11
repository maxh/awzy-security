use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-corp-portal");
    let url = std::env::var("AWZY_POSTGRES_URL").context("AWZY_POSTGRES_URL is not set")?;
    // `awzy-corp-portal migrate`: the one-off task deploy-corp.sh runs before the services roll
    // (ADR 0087). It migrates the registry proxy's schema too, which shares this database.
    if std::env::args().nth(1).as_deref() == Some("migrate") {
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await?;
        awzy_corp_portal::fleet::FleetStore::new(pool).migrate().await?;
        awzy_registry::tokens::TokenStore::migrate(&url, "registry").await?;
        tracing::info!("migrations applied");
        return Ok(());
    }
    // `awzy-corp-portal channel set <stable|fast> <chart version> <by>`: what CI runs, as a
    // one-off task, to move a release channel (ADR 0094).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let ["channel", "set", channel, version, by] = args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        let channel = awzy_corp_portal::fleet::Channel::from_name(channel)
            .with_context(|| format!("{channel:?} isn't a channel: stable or fast"))?;
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await?;
        let set = awzy_corp_portal::fleet::FleetStore::new(pool).set_channel(channel, version, by).await?;
        tracing::info!(channel = set.channel.as_str(), version = %set.chart_version, by = %set.changed_by, "channel moved");
        return Ok(());
    }
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await?;
    // The registry proxy's pull tokens live in its own schema of this database (ADR 0028).
    let pull_tokens = awzy_registry::tokens::TokenStore::connect(&url, "registry").await?;
    let mut store = awzy_corp_portal::fleet::FleetStore::new(pool).with_pull_tokens(pull_tokens);
    // Signs support session tokens (ADR 0095); without the key, staff can't sign in to one.
    match awzy_corp_portal::support::SupportSigner::from_env()? {
        Some(signer) => store = store.with_support_signer(signer),
        None => {
            tracing::warn!("AWZY_CORP_PORTAL_SUPPORT_SIGNING_KEY is not set; support sessions can't be signed in to")
        }
    }
    store.migrate_on_start().await?;
    // Fleet alerts to Slack (ADR 0043).
    let slack = std::env::var("AWZY_CORP_PORTAL_ALERTS_SLACK_WEBHOOK_URL")
        .ok()
        .and_then(|url| awzy_corp_portal::alerts::SlackWebhook::from_url(url.trim()));
    let corp_url = std::env::var("AWZY_CORP_PORTAL_PUBLIC_URL").ok().map(|u| u.trim_end_matches('/').to_string());
    tokio::spawn(awzy_corp_portal::alerts::run(store.clone(), slack, corp_url));
    // The built frontend carries the queries it sends by ID (ADR 0090). The corp portal also runs
    // query text: it holds no client data, and staff point local frontends at production (ADR 0024).
    let frontend = awzy_portal::frontend_dir_from_env();
    let persisted = awzy_portal::PersistedQueries::from_frontend(frontend.as_deref(), false)?;
    let mut app = awzy_corp_portal::router(awzy_corp_portal::auth::Auth::from_env()?, store, persisted);
    if let Some(dir) = frontend {
        app = awzy_portal::with_frontend(app, &dir);
    }
    awzy_server::serve("awzy-corp-portal", app).await
}

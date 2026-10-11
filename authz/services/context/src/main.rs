use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-context");
    let url = std::env::var("AWZY_POSTGRES_URL").context("AWZY_POSTGRES_URL is not set")?;
    // `awzy-context migrate`: the release Job (ADR 0083).
    if awzy_migrations::requested() {
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await?;
        awzy_context::store::ContextStore::new(pool).migrate().await?;
        tracing::info!("migrations applied");
        return Ok(());
    }
    // The bearer token the client's IdP uses for SCIM. Generate a long random one and set it in
    // the IdP's provisioning settings too.
    let scim_token = std::env::var("AWZY_SCIM_TOKEN").context("AWZY_SCIM_TOKEN is not set")?;
    anyhow::ensure!(scim_token.len() >= 32, "AWZY_SCIM_TOKEN must be at least 32 characters");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(8).connect(&url).await?;
    let store = awzy_context::store::ContextStore::new(pool);
    store.migrate_on_start().await?;
    // Wakes PDPs waiting for principals or resources to change (ADR 0077).
    tokio::spawn(store.clone().follow_changes());
    // Places new and changed resources in a sensitivity tier (ADR 0047).
    tokio::spawn(awzy_context::classify::run(store.clone(), std::time::Duration::from_secs(5)));
    // Who may call what (ADR 0055); SCIM keeps the IdP's token.
    let access = awzy_context::Access::from_env()?;
    awzy_server::serve("awzy-context", awzy_context::router(store, &scim_token, &access)).await
}

//! awzy-reasoner. ClickHouse at AWZY_CLICKHOUSE_URL as AWZY_CLICKHOUSE_USER (default
//! `awzy_reasoner`, read-only) with AWZY_CLICKHOUSE_PASSWORD, database AWZY_CLICKHOUSE_DATABASE
//! (default `awzy`); awzy-context at AWZY_CONTEXT_URL; scores every AWZY_REASONER_INTERVAL_SECS
//! (default 900). With AWZY_CHALLENGE_ADDR and AWZY_POLICY_ADDR, it also contains people when the
//! day's evidence adds up (ADR 0066).

use std::time::Duration;

use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-reasoner");
    let env = |k: &str| std::env::var(k).with_context(|| format!("{k} is not set"));
    let or = |k: &str, default: &str| std::env::var(k).unwrap_or_else(|_| default.into());
    let clickhouse = clickhouse::Client::default()
        .with_url(env("AWZY_CLICKHOUSE_URL")?)
        .with_user(or("AWZY_CLICKHOUSE_USER", "awzy_reasoner"))
        .with_password(env("AWZY_CLICKHOUSE_PASSWORD")?)
        .with_database(or("AWZY_CLICKHOUSE_DATABASE", "awzy"));
    let context = awzy_reasoner::context::ContextClient::new(
        &env("AWZY_CONTEXT_URL")?,
        awzy_service_auth::TokenSource::from_env(),
    );
    let interval = Duration::from_secs(or("AWZY_REASONER_INTERVAL_SECS", "900").parse()?);
    let containments = match (std::env::var("AWZY_CHALLENGE_ADDR"), std::env::var("AWZY_POLICY_ADDR")) {
        (Ok(challenge), Ok(policy)) => Some(awzy_reasoner::contain::Containments::connect(&challenge, &policy)?),
        _ => {
            tracing::warn!("AWZY_CHALLENGE_ADDR or AWZY_POLICY_ADDR is not set; nobody is contained automatically");
            None
        }
    };
    tokio::spawn(awzy_reasoner::Reasoner { clickhouse, context, containments }.run(interval));
    awzy_server::serve("awzy-reasoner", awzy_reasoner::router()).await
}

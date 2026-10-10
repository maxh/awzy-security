use std::{sync::Arc, time::Duration};

use anyhow::Context;
use awzy_challenge::{approvals::Approvals, ChallengeStore, Feed, Lifetimes};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-challenge");
    let url = std::env::var("AWZY_POSTGRES_URL").context("AWZY_POSTGRES_URL is not set")?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&url).await?;
    let defaults = Lifetimes::default();
    let lifetimes = Lifetimes {
        pending: secs("AWZY_CHALLENGE_PENDING_SECS")?.unwrap_or(defaults.pending),
        grant: secs("AWZY_CHALLENGE_GRANT_SECS")?.unwrap_or(defaults.grant),
    };
    let store = Arc::new(ChallengeStore::new(pool, lifetimes));
    // TODO: run migrations from a release Job instead of at startup.
    store.migrate().await?;

    // Approvers are whoever the PDP allows to `approve` the resource.
    let approvals = match std::env::var("AWZY_PDP_ADDR") {
        Ok(endpoint) => Approvals::pdp(&endpoint)?,
        Err(_) => {
            tracing::warn!("AWZY_PDP_ADDR is not set; approvals are refused");
            Approvals::Off
        }
    };

    let feed = Arc::new(Feed::default());
    tokio::spawn(feed.clone().follow(store.clone()));
    // Not ready until the containment list is loaded, so PDPs never get an empty one by mistake.
    let ready = feed.clone();
    // Who may call which methods (ADR 0055).
    let access = awzy_challenge::Access::from_env()?;
    awzy_server::serve_with_readiness(
        "awzy-challenge",
        awzy_challenge::router(store, feed, Arc::new(approvals), &access),
        Arc::new(move || ready.is_loaded()),
    )
    .await
}

fn secs(name: &str) -> anyhow::Result<Option<Duration>> {
    match std::env::var(name) {
        Ok(v) => Ok(Some(Duration::from_secs(v.parse().with_context(|| format!("{name} must be whole seconds"))?))),
        Err(_) => Ok(None),
    }
}

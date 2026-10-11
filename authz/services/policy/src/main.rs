use std::sync::Arc;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-policy");
    let url = std::env::var("AWZY_POSTGRES_URL").context("AWZY_POSTGRES_URL is not set")?;
    let pool = PgPoolOptions::new().max_connections(6).connect(&url).await?;
    let mut store = awzy_policy::PolicyStore::new(pool);
    // `awzy-policy migrate`: the release Job (ADR 0083).
    if awzy_store::migrations::requested() {
        store.migrate().await?;
        tracing::info!("migrations applied");
        return Ok(());
    }
    match std::env::var("AWZY_POLICY_SIGNING_KEY_FILE") {
        Ok(path) => {
            let pem = std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?;
            let signer = awzy_signing::BundleSigner::from_pem(&pem)?;
            tracing::info!(key_id = signer.key_id(), "signing published bundles");
            store = store.with_signer(signer);
        }
        // TODO: a KMS-backed signer per cloud.
        Err(_) => tracing::warn!("AWZY_POLICY_SIGNING_KEY_FILE is not set; published bundles are unsigned"),
    }
    store.migrate_on_start().await?;
    let break_glass = match std::env::var("AWZY_POLICY_BREAK_GLASS_KEYS_FILE") {
        Ok(path) => {
            let keys = awzy_signing::BundleVerifier::from_pem_bundle(
                &std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?,
            )?;
            tracing::info!(keys = ?keys.key_ids().collect::<Vec<_>>(), "break-glass keys enrolled");
            Some(Arc::new(keys))
        }
        Err(_) => {
            tracing::warn!("AWZY_POLICY_BREAK_GLASS_KEYS_FILE is not set; break-glass rollback is off");
            None
        }
    };
    let store = Arc::new(store);
    // Wakes PDPs waiting for a new bundle or mode (ADR 0077).
    let following = store.clone();
    tokio::spawn(async move { following.follow_changes().await });
    // Who may call which methods (ADR 0055).
    let access = awzy_policy::Access::from_env()?;
    // awzy-challenge tells the security team of each break-glass rollback (ADR 0022).
    let reporter = match std::env::var("AWZY_CHALLENGE_ADDR") {
        Ok(addr) => {
            let channel = tonic::transport::Channel::from_shared(addr)?.connect_lazy();
            Some(awzy_policy::BreakGlassReporter::new(awzy_service_auth::grpc_channel(channel, "awzy-challenge")))
        }
        Err(_) => None,
    };
    awzy_server::serve("awzy-policy", awzy_policy::router(store, break_glass, reporter, &access)).await
}

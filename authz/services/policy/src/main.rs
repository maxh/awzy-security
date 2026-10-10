use std::sync::Arc;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-policy");
    let url = std::env::var("AWZY_POSTGRES_URL").context("AWZY_POSTGRES_URL is not set")?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&url).await?;
    let mut store = awzy_policy::PolicyStore::new(pool);
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
    // TODO: run migrations from a release Job instead of at startup.
    store.migrate().await?;
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
    awzy_server::serve("awzy-policy", awzy_policy::router(Arc::new(store), break_glass)).await
}

use std::time::Duration;

use std::sync::Arc;

use awzy_model::EnforcementMode;
use awzy_pdp::{AppState, DecisionLog, Enforcement, DEFAULT_DECISIONS_TOPIC};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-pdp");
    let pdp = awzy_pdp::shared(awzy_pdp::Pdp::with_defaults()?);
    // Deciding needs a service token from a workload in the cluster (ADR 0054).
    let mut state = AppState::new(pdp.clone()).with_service_auth(awzy_service_auth::Verifier::from_env("awzy-pdp")?);

    match std::env::var("AWZY_POLICY_ADDR") {
        Ok(endpoint) => {
            let enforcement = Arc::new(Enforcement::awaiting_sync());
            state = state.with_enforcement(enforcement.clone());
            let verifier = match std::env::var("AWZY_POLICY_VERIFY_KEY_FILE") {
                Ok(path) => Some(Arc::new(awzy_signing::BundleVerifier::from_pem(&std::fs::read_to_string(&path)?)?)),
                Err(_) => {
                    tracing::warn!("AWZY_POLICY_VERIFY_KEY_FILE is not set; policy bundle signatures are not verified");
                    None
                }
            };
            tokio::spawn(awzy_pdp::sync_policies(pdp, enforcement, verifier, endpoint, Duration::from_secs(10)));
        }
        Err(_) => {
            tracing::warn!("AWZY_POLICY_ADDR is not set; built-in default policies, monitor mode");
            state = state.with_enforcement(Arc::new(Enforcement::fixed(EnforcementMode::Monitor)));
        }
    }

    // People and groups (ADR 0031), and resources by ID (ADR 0044), from awzy-context. Without
    // it, enforcement points' entities are used as they are.
    if let Ok(endpoint) = std::env::var("AWZY_CONTEXT_ADDR") {
        let principals = Arc::new(awzy_pdp::principals::Principals::awaiting_sync());
        let resources = Arc::new(awzy_pdp::resources::Resources::from_context(endpoint.clone()));
        state = state.with_principals(principals.clone()).with_resources(resources);
        tokio::spawn(awzy_pdp::principals::sync_principals(principals, endpoint, Duration::from_secs(5)));
    }

    match std::env::var("AWZY_KAFKA_BROKERS") {
        Ok(brokers) => {
            let topic = std::env::var("AWZY_DECISIONS_TOPIC").unwrap_or_else(|_| DEFAULT_DECISIONS_TOPIC.into());
            let brokers = brokers.split(',').map(str::to_string).collect();
            state = state.with_log(DecisionLog::connect_kafka(brokers, &topic).await?);
        }
        Err(_) => tracing::warn!("AWZY_KAFKA_BROKERS is not set; decisions are not logged"),
    }

    // Not ready until the first sync loads the bundle and mode; checks are refused until then too.
    let ready = state.clone();
    awzy_server::serve_with_readiness("awzy-pdp", awzy_pdp::router(state), Arc::new(move || ready.is_ready())).await
}

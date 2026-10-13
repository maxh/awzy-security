use std::time::Duration;

use std::sync::Arc;

use awzy_model::EnforcementMode;
use awzy_pdp::{AppState, DecisionLog, Enforcement, DEFAULT_DECISIONS_TOPIC};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-pdp");
    let pdp = awzy_pdp::shared(awzy_pdp::Pdp::with_defaults()?);
    // Deciding needs a service token from a workload in the cluster (ADR 0055).
    let mut state = AppState::new(pdp.clone()).with_service_auth(awzy_service_auth::Verifier::from_env("awzy-pdp")?);
    let mut verifier = None;

    match std::env::var("AWZY_POLICY_ADDR") {
        Ok(endpoint) => {
            let enforcement = Arc::new(Enforcement::awaiting_sync());
            state = state.with_enforcement(enforcement.clone());
            // One or more public keys, so a new signing key is trusted before it signs (ADR 0092).
            verifier = match std::env::var("AWZY_POLICY_VERIFY_KEY_FILE") {
                Ok(path) => {
                    Some(Arc::new(awzy_signing::BundleVerifier::from_pem_bundle(&std::fs::read_to_string(&path)?)?))
                }
                Err(_) => {
                    tracing::warn!("AWZY_POLICY_VERIFY_KEY_FILE is not set; policy bundle signatures are not verified");
                    None
                }
            };
            let every = Duration::from_secs(10);
            tokio::spawn(awzy_pdp::sync_policies(pdp, enforcement, verifier.clone(), endpoint, every));
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
        // How long a cached resource stands in while awzy-context is down, by tier (ADR 0119).
        let limits = match std::env::var("AWZY_PDP_RESOURCES_MAX_STALE_SECS") {
            Ok(text) => awzy_pdp::resources::StaleLimits::parse(&text)?,
            Err(_) => awzy_pdp::resources::StaleLimits::default(),
        };
        let mut resources = awzy_pdp::resources::Resources::from_context(endpoint.clone()).with_stale_limits(limits);
        // A resource this PDP hasn't cached can come from a running PDP while awzy-context is down.
        if let Ok(peers) = std::env::var("AWZY_PDP_PEER_ADDR") {
            resources = resources.with_peers(peers);
        }
        let resources = Arc::new(resources);
        state = state.with_principals(principals.clone()).with_resources(resources.clone());
        // Both wait on awzy-context for changes rather than polling (ADR 0077).
        tokio::spawn(awzy_pdp::principals::sync_principals(principals, endpoint, Duration::from_secs(5)));
        tokio::spawn(resources.follow_changes());
    }

    // Containments and challenges (ADR 0054). Without it, challenge verdicts carry no
    // challenge to answer and no one is contained.
    if let Ok(endpoint) = std::env::var("AWZY_CHALLENGE_ADDR") {
        // How long the last list stands in when awzy-challenge can't be reached (ADR 0119).
        let max_stale = match std::env::var("AWZY_PDP_CONTAINMENTS_MAX_STALE_SECS") {
            Ok(secs) => Duration::from_secs(secs.parse()?),
            Err(_) => awzy_pdp::containments::DEFAULT_MAX_STALE,
        };
        let containments =
            Arc::new(awzy_pdp::containments::Containments::from_challenge(&endpoint)?.with_max_stale(max_stale));
        let resolver = Arc::new(awzy_pdp::challenges::ChallengeResolver::from_challenge(&endpoint)?);
        awzy_server::metrics::register(containments.clone());
        state = state.with_challenges(containments.clone(), resolver);
        tokio::spawn(containments.follow());
    }

    match awzy_kafka::Brokers::from_env("AWZY_KAFKA_BROKERS")? {
        Some(brokers) => {
            let topic = std::env::var("AWZY_DECISIONS_TOPIC").unwrap_or_else(|_| DEFAULT_DECISIONS_TOPIC.into());
            state = state.with_log(DecisionLog::connect_kafka(&brokers, &topic).await?);
            // Query plans have their own record and topic (ADR 0113).
            let plans =
                std::env::var("AWZY_PLANS_TOPIC").unwrap_or_else(|_| awzy_pdp::plan::DEFAULT_PLANS_TOPIC.into());
            state = state.with_plan_log(awzy_pdp::plan::PlanLog::connect_kafka(&brokers, &plans).await?);
        }
        None => tracing::warn!("AWZY_KAFKA_BROKERS is not set; decisions are not logged"),
    }

    awzy_server::metrics::register(state.metrics.clone());

    // A PDP starting while its sources are down takes what it's missing from a running one
    // (ADR 0119): the headless Service that lists ready PDPs, as `host:port`.
    match std::env::var("AWZY_PDP_PEER_ADDR") {
        Ok(peers) => {
            tokio::spawn(awzy_pdp::peer::start_from_peers(state.clone(), verifier, peers));
        }
        Err(_) => tracing::info!("AWZY_PDP_PEER_ADDR is not set; a PDP starts only from its sources"),
    }

    // Not ready until the first sync loads the bundle and mode; checks are refused until then too.
    let ready = state.clone();
    awzy_server::serve_with_readiness("awzy-pdp", awzy_pdp::router(state), Arc::new(move || ready.is_ready())).await
}

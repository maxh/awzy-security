//! awzy-reasoner. ClickHouse at AWZY_CLICKHOUSE_URL as AWZY_CLICKHOUSE_USER (default
//! `awzy_reasoner`, read-only) with AWZY_CLICKHOUSE_PASSWORD, database AWZY_CLICKHOUSE_DATABASE
//! (default `awzy`); awzy-context at AWZY_CONTEXT_URL; scores every AWZY_REASONER_INTERVAL_SECS
//! (default 900). With AWZY_CHALLENGE_ADDR and AWZY_POLICY_ADDR, it also contains people when the
//! day's evidence adds up (ADR 0066). AWZY_REASONER_INVESTIGATIONS=true investigates the people it
//! flags with Claude on Bedrock (AWZY_LLM_MODEL, AWZY_LLM_EFFORT, AWS_REGION), at most
//! AWZY_REASONER_INVESTIGATION_TOKENS a month (default 2,000,000), proposing rather than applying
//! containments for members of AWZY_REASONER_PRIVILEGED_GROUPS (default `it,security`; ADR 0071).

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
    let investigations = if or("AWZY_REASONER_INVESTIGATIONS", "false") == "true" {
        let router = std::sync::Arc::new(awzy_llm::ModelRouter::from_env().await?);
        // The investigations route as set in the client portal and kept by awzy-policy (ADR 0120).
        if let Ok(addr) = std::env::var("AWZY_POLICY_ADDR") {
            let policy = awzy_authz_proto::policy::v1::policy_service_client::PolicyServiceClient::new(
                awzy_service_auth::grpc_channel(
                    tonic::transport::Channel::from_shared(addr)?.connect_lazy(),
                    "awzy-policy",
                ),
            );
            let setting = std::sync::Arc::new(awzy_model_routing::RoutingSetting::new(router.clone()));
            tokio::spawn(setting.follow(policy, awzy_model_routing::verifier_from_env()?));
        }
        Some(awzy_reasoner::runs::Investigations {
            models: router,
            monthly_tokens: or("AWZY_REASONER_INVESTIGATION_TOKENS", "2000000").parse()?,
            privileged_groups: or("AWZY_REASONER_PRIVILEGED_GROUPS", "it,security")
                .split(',')
                .map(str::trim)
                .filter(|g| !g.is_empty())
                .map(str::to_string)
                .collect(),
        })
    } else {
        None
    };
    tokio::spawn(awzy_reasoner::Reasoner { clickhouse, context, containments, investigations }.run(interval));
    awzy_server::serve("awzy-reasoner", awzy_reasoner::router()).await
}

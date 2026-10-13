use std::{sync::Arc, time::Duration};

use anyhow::Context;
use awzy_challenge::{approvals::Approvals, keys::SecurityKeys, notify::Channel, ChallengeStore, Feed, Lifetimes};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-challenge");
    let url = std::env::var("AWZY_POSTGRES_URL").context("AWZY_POSTGRES_URL is not set")?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&url).await?;
    // `awzy-challenge migrate`: the release Job (ADR 0083).
    if awzy_migrations::requested() {
        ChallengeStore::new(pool, Lifetimes::default()).migrate().await?;
        tracing::info!("migrations applied");
        return Ok(());
    }
    let defaults = Lifetimes::default();
    let lifetimes = Lifetimes {
        pending: secs("AWZY_CHALLENGE_PENDING_SECS")?.unwrap_or(defaults.pending),
        grant: secs("AWZY_CHALLENGE_GRANT_SECS")?.unwrap_or(defaults.grant),
    };
    let mut store = ChallengeStore::new(pool.clone(), lifetimes);
    let mut worker = None;

    // Telling people what's waiting for them: in the client's Slack (ADR 0073), and by email
    // through their SMTP relay (ADR 0122).
    let slack = match std::env::var("AWZY_CHALLENGE_SLACK_TOKEN") {
        Ok(token) if !token.is_empty() => {
            let channel = std::env::var("AWZY_CHALLENGE_SLACK_SECURITY_CHANNEL")
                .context("AWZY_CHALLENGE_SLACK_SECURITY_CHANNEL is required with AWZY_CHALLENGE_SLACK_TOKEN")?;
            let api = std::env::var("AWZY_CHALLENGE_SLACK_API").unwrap_or_else(|_| "https://slack.com/api".into());
            Some(awzy_challenge::slack::Slack::new(&api, &token, &channel))
        }
        _ => None,
    };
    let email = match std::env::var("AWZY_CHALLENGE_EMAIL_RELAY_URL") {
        Ok(relay) if !relay.is_empty() => {
            let from = std::env::var("AWZY_CHALLENGE_EMAIL_FROM")
                .context("AWZY_CHALLENGE_EMAIL_FROM is required with AWZY_CHALLENGE_EMAIL_RELAY_URL")?;
            let security = std::env::var("AWZY_CHALLENGE_EMAIL_SECURITY_ADDRESS")
                .context("AWZY_CHALLENGE_EMAIL_SECURITY_ADDRESS is required with AWZY_CHALLENGE_EMAIL_RELAY_URL")?;
            Some(awzy_challenge::email::Email::new(&relay, &from, &security)?)
        }
        _ => None,
    };
    let channels: Vec<Channel> =
        [slack.as_ref().map(|_| Channel::Slack), email.as_ref().map(|_| Channel::Email)].into_iter().flatten().collect();
    if channels.is_empty() {
        tracing::warn!(
            "neither AWZY_CHALLENGE_SLACK_TOKEN nor AWZY_CHALLENGE_EMAIL_RELAY_URL is set; nobody is told about challenges"
        );
    } else {
        let portal_url = std::env::var("AWZY_CHALLENGE_PORTAL_URL")
            .context("AWZY_CHALLENGE_PORTAL_URL is required for Slack or email notifications")?;
        // A person's SCIM email, for those whose userName isn't one (#65).
        let contacts = std::env::var("AWZY_CHALLENGE_CONTEXT_ADDR")
            .ok()
            .filter(|a| !a.is_empty())
            .map(|a| awzy_context_api::ContextApi::new(&a, awzy_service_auth::TokenSource::from_env()));
        tracing::info!(?channels, "telling people what's waiting for them");
        worker = Some(awzy_challenge::notify::Worker { pool, slack, email, portal_url, contacts });
        store = store.with_notifications(channels);
    }
    let store = Arc::new(store);
    store.migrate_on_start().await?;
    if let Some(worker) = worker {
        tokio::spawn(worker.run());
    }

    // Approvers are whoever the PDP allows to `approve` the resource.
    let approvals = match std::env::var("AWZY_PDP_ADDR") {
        Ok(endpoint) => Approvals::pdp(&endpoint)?,
        Err(_) => {
            tracing::warn!("AWZY_PDP_ADDR is not set; approvals are refused");
            Approvals::Off
        }
    };

    // Security keys (ADR 0059): the RP ID is the portal's hostname, and the origins its URLs.
    let keys = match std::env::var("AWZY_CHALLENGE_WEBAUTHN_RP_ID") {
        Ok(rp_id) => {
            let origins = std::env::var("AWZY_CHALLENGE_WEBAUTHN_ORIGINS")
                .context("AWZY_CHALLENGE_WEBAUTHN_ORIGINS is required with AWZY_CHALLENGE_WEBAUTHN_RP_ID")?;
            let origins: Vec<String> =
                origins.split(',').map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect();
            tracing::info!(%rp_id, ?origins, "security keys on");
            let rp = awzy_challenge::webauthn::RelyingParty::new(rp_id, origins);
            Some(Arc::new(SecurityKeys::new(store.pool().clone(), rp, store.clone())))
        }
        Err(_) => {
            tracing::warn!("AWZY_CHALLENGE_WEBAUTHN_RP_ID is not set; security keys are off");
            None
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
        awzy_challenge::router(store, feed, Arc::new(approvals), keys, &access),
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

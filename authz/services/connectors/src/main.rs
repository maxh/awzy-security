use std::sync::Arc;

use anyhow::Context;
use awzy_connectors::{
    context::ContextClient,
    drive::DriveMetrics,
    http::Http,
    jira::JiraMetrics,
    supervisor::{CredentialFiles, Supervisor},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-connectors");
    let context_url = std::env::var("AWZY_CONTEXT_URL").context("AWZY_CONTEXT_URL is not set")?;
    let http = Http::default();
    let context = ContextClient::new(&context_url, awzy_service_auth::TokenSource::from_env());
    // Where External Secrets keeps each credential current (ADR 0104).
    let file = |k: &str, default: &str| std::env::var(k).unwrap_or_else(|_| default.to_string());
    let files = CredentialFiles {
        drive_key: file("AWZY_CONNECTORS_DRIVE_KEY_FILE", "/etc/awzy/drive/key.json"),
        jira_token: file("AWZY_CONNECTORS_JIRA_TOKEN_FILE", "/etc/awzy/jira/token"),
    };
    // Each renders nothing until its connector has run once.
    awzy_server::metrics::register(Arc::new(DriveMetrics));
    awzy_server::metrics::register(Arc::new(JiraMetrics));
    // Google Drive (ADR 0069) and Jira (ADR 0070), as the client portal configures them.
    let api = awzy_context_api::ContextApi::new(&context_url, awzy_service_auth::TokenSource::from_env());
    tokio::spawn(Supervisor::new(context, api, http, files).run());
    awzy_server::serve("awzy-connectors", awzy_connectors::router()).await
}

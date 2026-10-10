use std::sync::Arc;

use anyhow::Context;
use awzy_connectors::{
    context::ContextClient,
    drive::{Credentials, DriveConfig, DriveMetrics, DriveSync},
    google::Auth,
    http::Http,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-connectors");

    // Google Drive (ADR 0069), when AWZY_CONNECTORS_DRIVE_SUBJECT is set.
    if let Some(cfg) = DriveConfig::from_env()? {
        let key_file = std::env::var("AWZY_CONNECTORS_DRIVE_KEY_FILE")
            .context("AWZY_CONNECTORS_DRIVE_KEY_FILE is needed for Drive")?;
        let context_url = std::env::var("AWZY_CONTEXT_URL").context("AWZY_CONTEXT_URL is needed for Drive")?;
        let http = Http::default();
        let credentials = Arc::new(Credentials::ServiceAccount(Box::new(Auth::from_file(&key_file, http.clone())?)));
        let context = ContextClient::new(&context_url, http.clone(), awzy_service_auth::TokenSource::from_env());
        awzy_server::metrics::register(Arc::new(DriveMetrics));
        tracing::info!(subject = %cfg.subject, domain = %cfg.domain, "syncing Google Drive");
        tokio::spawn(DriveSync::new(cfg, credentials, http, context).run());
    }

    awzy_server::serve("awzy-connectors", awzy_connectors::router()).await
}

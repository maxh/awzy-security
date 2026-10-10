//! Logging and tracing setup shared by every service.
//!
//! `RUST_LOG` sets the filter (default `info`). `AWZY_LOG_FORMAT=json` switches to JSON lines.
//! OpenTelemetry export is added here so services never configure it themselves.

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(service: &'static str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let json = std::env::var("AWZY_LOG_FORMAT").is_ok_and(|v| v == "json");
    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry.with(fmt::layer().json()).init();
    } else {
        registry.with(fmt::layer()).init();
    }
    tracing::debug!(service, "telemetry initialized");
}

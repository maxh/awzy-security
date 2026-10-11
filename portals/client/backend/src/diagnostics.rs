//! The diagnostics bundle: the install's health and configuration, for an admin to review and
//! send to awzy support when the status page isn't enough (platform plan, "Operating model").
//!
//! It holds no client data (ADR 0009): no people, resources, decisions, or policy, and no
//! setting's value unless it's on a short list known to be harmless. Every other setting shows
//! only that it's set, so secrets, URLs, and hostnames never leave in it.

use std::collections::BTreeMap;

use async_graphql::{Context, Object, Result};
use awzy_portal::JsonString;
use chrono::Utc;
use serde_json::json;

use crate::{
    access::{require, PortalAction, PortalArea},
    status, Services,
};

/// Settings whose values the bundle includes. Sorted (ADR 0053).
const SHOWN: &[&str] = &[
    "AWZY_CLICKHOUSE_DATABASE",
    "AWZY_IMAGE_DIGEST",
    "AWZY_LOG_FORMAT",
    "AWZY_POSTGRES_MIGRATE",
    "AWZY_RELEASE_VERSION",
];

#[derive(Default)]
pub struct DiagnosticsQuery;

#[Object]
impl DiagnosticsQuery {
    /// A diagnostics bundle as JSON: each service's status, the portal's release, and which
    /// settings it was started with, with no client data. For those who may administer the
    /// enforcement area, to review before sending it to awzy support.
    async fn diagnostics_bundle(&self, ctx: &Context<'_>) -> Result<JsonString> {
        require(ctx, PortalAction::Administer, PortalArea::Enforcement).await?;
        let services = ctx.data::<Services>()?;
        let bundle = json!({
            "generatedAt": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "portal": awzy_server::version(),
            "services": status::check(services).await,
            "features": { "awzyBot": services.bot.is_some() },
            "settings": settings(std::env::vars()),
        });
        // Pretty, since the admin reads it before sending it.
        Ok(JsonString::parse_text(serde_json::to_string_pretty(&bundle)?)?)
    }
}

/// awzy's settings in `env`, by name: the value for those in [`SHOWN`], "set" for the rest.
fn settings(env: impl Iterator<Item = (String, String)>) -> BTreeMap<String, String> {
    env.filter(|(k, _)| k.starts_with("AWZY_"))
        .map(|(k, v)| {
            let v = if SHOWN.contains(&k.as_str()) { v } else { "set".to_string() };
            (k, v)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundle_names_settings_but_never_shows_a_secret() {
        let env = [
            ("AWZY_CLICKHOUSE_PASSWORD", "hunter2"),
            ("AWZY_AUTH_ISSUER", "https://idp.example.com"),
            ("AWZY_LOG_FORMAT", "json"),
            ("HOME", "/root"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let shown = settings(env.into_iter());
        assert_eq!(
            shown.into_iter().collect::<Vec<_>>(),
            [("AWZY_AUTH_ISSUER", "set"), ("AWZY_CLICKHOUSE_PASSWORD", "set"), ("AWZY_LOG_FORMAT", "json")]
                .map(|(k, v)| (k.to_string(), v.to_string()))
        );
    }
}

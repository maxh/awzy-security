//! MCP for awzy's own agents, at `/mcp` (streamable HTTP): read-only tools over what the corp
//! portal already holds, the installs' registrations, heartbeats, and release channels. Nothing
//! here reaches into an install, and nothing holds client data (ADR 0009).
//!
//! Agents sign in with an MCP token a staff member mints in the corp portal (`mcpToken`), which
//! works only here; each call is logged with the staff member it's for.

use std::sync::Arc;

use awzy_model::InstallId;
use axum::{http::request::Parts, Router};
use chrono::{DateTime, SecondsFormat, Utc};
use rmcp::{
    handler::server::{
        tool::Extension,
        wrapper::{Json, Parameters},
    },
    schemars::JsonSchema,
    tool, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData,
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::Staff,
    fleet::{FleetStore, InstallRecord},
};

#[derive(Clone)]
pub struct CorpPortalMcp {
    store: FleetStore,
}

/// Serves MCP at `/mcp`. `allowed_hosts` are the Host headers to accept (DNS-rebinding
/// protection); empty keeps rmcp's default of loopback hosts only.
pub fn router(store: FleetStore, allowed_hosts: Vec<String>) -> Router {
    let mut config = StreamableHttpServerConfig::default();
    config.json_response = true;
    // Stateless, like the client portal's: every tool is one request and response.
    config.legacy_session_mode = false;
    if !allowed_hosts.is_empty() {
        config.allowed_hosts = allowed_hosts;
    }
    let service = StreamableHttpService::new(
        move || Ok(CorpPortalMcp { store: store.clone() }),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    Router::new().nest_service("/mcp", service)
}

fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn internal(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct InstallSummary {
    /// `install_<uuid>`.
    pub id: String,
    pub client: String,
    pub environment: String,
    pub cloud: String,
    pub region: String,
    /// stable, fast, or pinned.
    pub channel: String,
    pub pinned_version: Option<String>,
    /// ok, degraded, not_reporting, or never_reported.
    pub health: String,
    pub last_seen_at: Option<String>,
    /// The chart version its last heartbeat named.
    pub chart_version: Option<String>,
    pub retired: bool,
}

fn summary(r: &InstallRecord, now: DateTime<Utc>) -> InstallSummary {
    InstallSummary {
        id: r.id.to_string(),
        client: r.client.clone(),
        environment: r.environment.clone(),
        cloud: r.cloud.clone(),
        region: r.region.clone(),
        channel: r.channel.as_str().to_string(),
        pinned_version: r.pinned_version.clone(),
        health: r.health(now).as_str().to_string(),
        last_seen_at: r.last_seen_at.map(rfc3339),
        chart_version: r.last_heartbeat.as_ref().and_then(|h| h.chart_version.clone()),
        retired: r.retired_at.is_some(),
    }
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct InstallList {
    pub installs: Vec<InstallSummary>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct InstallParams {
    /// The install's ID, `install_<uuid>`.
    pub install_qid: String,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct InstallDetail {
    pub install: InstallSummary,
    /// Its last heartbeat as it sent it: services, versions, readiness, and aggregate metrics.
    /// The heartbeat's fixed schema holds no client data.
    pub last_heartbeat: Option<serde_json::Value>,
    /// Each chart version it reported, newest first, with when it was first seen.
    pub versions: Vec<VersionSeen>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct VersionSeen {
    pub chart_version: String,
    pub first_seen_at: String,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct Channels {
    /// The chart version each release channel names.
    pub channels: Vec<ChannelOut>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ChannelOut {
    pub channel: String,
    pub chart_version: String,
    pub changed_by: String,
    pub changed_at: String,
}

#[tool_router(server_handler)]
impl CorpPortalMcp {
    #[tool(
        name = "list_installs",
        description = "Every registered install: client, environment, cloud and region, release channel, health (ok, degraded, not_reporting, never_reported), when it last reported, and the chart version it runs. Read-only."
    )]
    async fn list_installs(&self, Extension(parts): Extension<Parts>) -> Result<Json<InstallList>, ErrorData> {
        log(&parts, "list_installs");
        let now = Utc::now();
        let installs = self.store.all_installs().await.map_err(internal)?;
        Ok(Json(InstallList { installs: installs.iter().map(|r| summary(r, now)).collect() }))
    }

    #[tool(
        name = "get_install",
        description = "One install: its registration and health, its last heartbeat (services, versions, readiness, and aggregate metrics such as PDP p99, Kafka lag, and freshness), and the chart versions it has run. Read-only."
    )]
    async fn get_install(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<InstallParams>,
    ) -> Result<Json<InstallDetail>, ErrorData> {
        log(&parts, "get_install");
        let id: InstallId =
            p.install_qid.parse().map_err(|e| ErrorData::invalid_params(format!("install_qid: {e}"), None))?;
        let record = self
            .store
            .get(id)
            .await
            .map_err(internal)?
            .ok_or_else(|| ErrorData::invalid_params(format!("no install {id}"), None))?;
        let versions = self.store.versions(id).await.map_err(internal)?;
        Ok(Json(InstallDetail {
            install: summary(&record, Utc::now()),
            last_heartbeat: record.last_heartbeat.as_ref().and_then(|h| serde_json::to_value(h).ok()),
            versions: versions
                .into_iter()
                .map(|(chart_version, at)| VersionSeen { chart_version, first_seen_at: rfc3339(at) })
                .collect(),
        }))
    }

    #[tool(
        name = "list_release_channels",
        description = "The chart version each release channel (stable, fast) names, and who moved it when. Read-only."
    )]
    async fn list_release_channels(&self, Extension(parts): Extension<Parts>) -> Result<Json<Channels>, ErrorData> {
        log(&parts, "list_release_channels");
        let channels = self.store.channel_versions().await.map_err(internal)?;
        Ok(Json(Channels {
            channels: channels
                .into_iter()
                .map(|c| ChannelOut {
                    channel: c.channel.as_str().to_string(),
                    chart_version: c.chart_version,
                    changed_by: c.changed_by,
                    changed_at: rfc3339(c.changed_at),
                })
                .collect(),
        }))
    }
}

/// Which staff member's agent called which tool.
fn log(parts: &Parts, tool: &str) {
    let staff = parts.extensions.get::<Staff>().map(|s| s.username.as_str()).unwrap_or("unknown");
    tracing::info!(staff, tool, "corp portal MCP call");
}

/// Minting an agent's MCP token.
#[derive(Default)]
pub struct McpTokenMutation;

#[async_graphql::Object]
impl McpTokenMutation {
    /// A token for an agent of yours on the corp portal's MCP endpoint (`/mcp`): read-only
    /// tools, for 30 days, and nothing else here. Shown once.
    async fn mcp_token(&self, ctx: &async_graphql::Context<'_>) -> async_graphql::Result<String> {
        let staff = ctx.data::<Staff>()?;
        let auth = ctx.data::<crate::auth::Auth>()?;
        tracing::info!(staff = %staff.username, "MCP token issued");
        Ok(auth.issue_mcp_token(staff))
    }
}

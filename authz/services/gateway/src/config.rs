//! The gateway's configuration, from a JSON file (AWZY_GATEWAY_CONFIG).
//!
//! Upstream MCP servers, tool sensitivity, and the service account registry live here until
//! awzy-context owns them; then the PDP will load those entities itself.

use std::collections::HashMap;

use awzy_identity::{ServiceAccountConfig, TokenConfig};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    pub token: TokenConfig,
    pub upstreams: Vec<UpstreamConfig>,
    /// Registered service accounts. A token naming any other service account is denied.
    #[serde(default)]
    pub service_accounts: Vec<ServiceAccountConfig>,
    /// `config` (the list above) or `awzy-context`: the PDP takes service accounts from
    /// awzy-context and denies unregistered ones (ADR 0033).
    #[serde(default)]
    pub service_account_registry: Registry,
    /// What happens to tool calls while the PDP can't be reached (ADR 0030).
    #[serde(default)]
    pub outage: OutageConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Registry {
    #[default]
    Config,
    AwzyContext,
}

/// Tiers that may never fail open, whatever the config says.
pub const ALWAYS_FAIL_CLOSED: &[&str] = &["confidential", "crown_jewels"];

/// While the PDP is unreachable, an enforcing gateway denies every call except one to a tool
/// listed by name in a fail-open tier, by a caller the PDP allowed to make that same call
/// within the last `cached_allow_ttl_secs`. Tools without their own entry fail closed.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutageConfig {
    /// Sensitivity tiers whose tools may fail open, e.g. `["public", "internal"]`. Default: none.
    #[serde(default)]
    pub fail_open_tiers: Vec<String>,
    /// How long a cached allow can be replayed. Default 5 minutes, at most an hour.
    #[serde(default = "default_ttl")]
    pub cached_allow_ttl_secs: u64,
}

fn default_ttl() -> u64 {
    300
}

impl Default for OutageConfig {
    fn default() -> Self {
        Self { fail_open_tiers: vec![], cached_allow_ttl_secs: default_ttl() }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamConfig {
    /// The path segment agents use: `/mcp/{name}`. Also the McpServer entity ID.
    pub name: String,
    /// The upstream MCP server's streamable HTTP endpoint, e.g. `http://drive-mcp:8080/mcp`.
    pub url: String,
    /// Sensitivity of tools not listed in `tools`.
    #[serde(default = "default_sensitivity")]
    pub default_sensitivity: String,
    /// Sensitivity per tool name.
    #[serde(default)]
    pub tools: HashMap<String, String>,
    /// Tools that act on one resource awzy-context knows, by the argument that names it
    /// (ADR 0048). A call to one is checked twice: `call` on the tool, then `action` on
    /// `Resource::"<argument>"`, which the PDP loads by ID. Both must pass.
    #[serde(default)]
    pub resource_tools: HashMap<String, ResourceTool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceTool {
    /// The tool argument holding the resource's ID, e.g. `"id"`.
    pub argument: String,
    /// `read` (default) or `write`.
    #[serde(default = "default_action")]
    pub action: String,
}

fn default_action() -> String {
    "read".into()
}

impl UpstreamConfig {
    pub fn sensitivity(&self, tool: &str) -> &str {
        self.tools.get(tool).map_or(&self.default_sensitivity, String::as_str)
    }

    /// The tool's tier only if it is listed by name. During an outage unlisted tools fail closed.
    pub fn listed_sensitivity(&self, tool: &str) -> Option<&str> {
        self.tools.get(tool).map(String::as_str)
    }
}

fn default_sensitivity() -> String {
    "internal".into()
}

impl GatewayConfig {
    pub fn upstream(&self, name: &str) -> Option<&UpstreamConfig> {
        self.upstreams.iter().find(|u| u.name == name)
    }

    /// Rejects an outage policy that would let high-sensitivity tools fail open.
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(t) = self.outage.fail_open_tiers.iter().find(|t| ALWAYS_FAIL_CLOSED.contains(&t.as_str())) {
            anyhow::bail!("outage.fail_open_tiers can't include {t:?}: {ALWAYS_FAIL_CLOSED:?} always fail closed");
        }
        anyhow::ensure!(self.outage.cached_allow_ttl_secs <= 3600, "outage.cached_allow_ttl_secs is at most 3600");
        for up in &self.upstreams {
            for (tool, rt) in &up.resource_tools {
                anyhow::ensure!(
                    ["read", "write"].contains(&rt.action.as_str()) && !rt.argument.is_empty(),
                    "upstream {:?}, resource tool {tool:?}: action is read or write, and argument names one",
                    up.name
                );
            }
        }
        anyhow::ensure!(
            self.service_account_registry == Registry::Config || self.service_accounts.is_empty(),
            "with service_account_registry awzy-context, service accounts are registered there, not in service_accounts"
        );
        Ok(())
    }
}

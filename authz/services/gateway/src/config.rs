//! The gateway's configuration, from a JSON file (AWZY_GATEWAY_CONFIG).
//!
//! Upstream MCP servers and the service account registry live here. Each tool's tier and which
//! tiers fail open during an outage come from the client portal, through awzy-policy (ADR 0089).

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
    /// Where agents reach the gateway, e.g. `https://mcp.example.com`: for the OAuth metadata MCP
    /// clients find the IdP with (ADR 0081). Unset, the gateway publishes none.
    #[serde(default)]
    pub public_url: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Registry {
    #[default]
    Config,
    AwzyContext,
}

/// While the PDP is unreachable, an enforcing gateway denies every call except one to a tool
/// classified by name in a fail-open tier (the outage fallback setting, ADR 0089), by a caller
/// the PDP allowed to make that same call within the last `cached_allow_ttl_secs`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutageConfig {
    /// How long a cached allow can be replayed. Default 5 minutes, at most an hour.
    #[serde(default = "default_ttl")]
    pub cached_allow_ttl_secs: u64,
}

fn default_ttl() -> u64 {
    300
}

impl Default for OutageConfig {
    fn default() -> Self {
        Self { cached_allow_ttl_secs: default_ttl() }
    }
}

/// Refuses fields it doesn't know, so a config still classifying tools here fails at start rather
/// than being ignored: tiers are set in the client portal now (ADR 0089).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamConfig {
    /// The path segment agents use: `/mcp/{name}`. Also the McpServer entity ID.
    pub name: String,
    /// The upstream MCP server's streamable HTTP endpoint, e.g. `http://drive-mcp:8080/mcp`.
    pub url: String,
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

impl GatewayConfig {
    pub fn upstream(&self, name: &str) -> Option<&UpstreamConfig> {
        self.upstreams.iter().find(|u| u.name == name)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_still_classifying_tools_is_refused() {
        let up = serde_json::json!({ "name": "drive", "url": "http://drive/mcp" });
        assert!(serde_json::from_value::<UpstreamConfig>(up).is_ok());
        let stale =
            serde_json::json!({ "name": "drive", "url": "http://drive/mcp", "tools": { "search": "internal" } });
        assert!(serde_json::from_value::<UpstreamConfig>(stale).is_err());
        let stale = serde_json::json!({ "fail_open_tiers": ["internal"] });
        assert!(serde_json::from_value::<OutageConfig>(stale).is_err());
    }
}

//! What enforcement points do while awzy-pdp can't be reached (ADR 0030, 0089): each MCP tool's
//! sensitivity tier, and which tiers may replay a recent allow. Set in the client portal, kept and
//! signed by awzy-policy, and verified by awzy-gateway before it uses it.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Sensitivity tiers, least sensitive first.
pub const TIERS: &[&str] = &["public", "internal", "confidential", "crown_jewels"];

/// Tiers that never fail open, whatever the setting says.
pub const ALWAYS_FAIL_CLOSED: &[&str] = &["confidential", "crown_jewels"];

/// The tier of anything not classified, so a tool an upstream adds isn't open to everyone before
/// someone classifies it.
pub const UNCLASSIFIED_TIER: &str = "confidential";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutageFallback {
    /// Each classified tool's tier, by `<server>/<tool>`.
    pub tools: BTreeMap<String, String>,
    /// The tier of a server's tools that aren't listed in `tools`, by server name. Checks use it;
    /// during an outage those tools fail closed anyway.
    pub servers: BTreeMap<String, String>,
    /// Tiers whose listed tools may replay a recent allow during an outage. Never one of
    /// [`ALWAYS_FAIL_CLOSED`].
    pub fail_open_tiers: BTreeSet<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FallbackError {
    #[error("{0:?} isn't a tier: use one of {TIERS:?}")]
    UnknownTier(String),
    #[error("{0} always fails closed")]
    AlwaysClosed(String),
    #[error("{0:?} isn't a tool: name it as <server>/<tool>")]
    BadTool(String),
    #[error("{0:?} isn't a server name")]
    BadServer(String),
}

impl OutageFallback {
    /// For a check: the tool's own tier, else its server's, else [`UNCLASSIFIED_TIER`].
    pub fn tier(&self, server: &str, tool: &str) -> &str {
        self.listed_tier(server, tool)
            .or_else(|| self.servers.get(server).map(String::as_str))
            .unwrap_or(UNCLASSIFIED_TIER)
    }

    /// During an outage: the tool's tier only if it's listed by name. Anything else fails closed.
    pub fn listed_tier(&self, server: &str, tool: &str) -> Option<&str> {
        self.tools.get(&format!("{server}/{tool}")).map(String::as_str)
    }

    /// Whether a listed tool of `tier` may replay a recent allow during an outage.
    pub fn fails_open(&self, tier: &str) -> bool {
        self.fail_open_tiers.contains(tier) && !ALWAYS_FAIL_CLOSED.contains(&tier)
    }

    pub fn validate(&self) -> Result<(), FallbackError> {
        let tier = |t: &String| {
            if TIERS.contains(&t.as_str()) {
                Ok(())
            } else {
                Err(FallbackError::UnknownTier(t.clone()))
            }
        };
        for (tool, t) in &self.tools {
            let parts: Vec<&str> = tool.split('/').collect();
            if parts.len() != 2 || parts.iter().any(|p| !is_name(p)) {
                return Err(FallbackError::BadTool(tool.clone()));
            }
            tier(t)?;
        }
        for (server, t) in &self.servers {
            if !is_name(server) {
                return Err(FallbackError::BadServer(server.clone()));
            }
            tier(t)?;
        }
        for t in &self.fail_open_tiers {
            tier(t)?;
            if ALWAYS_FAIL_CLOSED.contains(&t.as_str()) {
                return Err(FallbackError::AlwaysClosed(t.clone()));
            }
        }
        Ok(())
    }

    /// The text whose hash is signed: one sorted line per entry, so the same setting is always
    /// the same bytes. Names can't hold whitespace or `/` beyond the one in a tool, so it's
    /// unambiguous.
    pub fn content(&self) -> String {
        let mut text = String::new();
        for (server, t) in &self.servers {
            text.push_str(&format!("server {server} {t}\n"));
        }
        for (tool, t) in &self.tools {
            text.push_str(&format!("tool {tool} {t}\n"));
        }
        for t in &self.fail_open_tiers {
            text.push_str(&format!("fail_open {t}\n"));
        }
        text
    }
}

/// A server or tool name: what MCP servers use, and nothing that could split a line.
fn is_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fallback() -> OutageFallback {
        OutageFallback {
            tools: [("drive/search", "internal"), ("admin/reset_password", "crown_jewels")]
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .into(),
            servers: [("admin".to_string(), "crown_jewels".to_string())].into(),
            fail_open_tiers: ["internal".to_string()].into(),
        }
    }

    #[test]
    fn a_tool_has_its_own_tier_else_its_servers_else_confidential() {
        let f = fallback();
        assert_eq!(f.tier("drive", "search"), "internal");
        assert_eq!(f.tier("admin", "export_directory"), "crown_jewels");
        assert_eq!(f.tier("drive", "delete_file"), "confidential");
        // Only a tool listed by name has a tier during an outage.
        assert_eq!(f.listed_tier("admin", "export_directory"), None);
        assert!(f.fails_open("internal") && !f.fails_open("crown_jewels"));
    }

    #[test]
    fn high_sensitivity_tiers_never_fail_open_and_names_are_checked() {
        assert_eq!(fallback().validate(), Ok(()));
        let mut f = fallback();
        f.fail_open_tiers.insert("confidential".into());
        assert_eq!(f.validate(), Err(FallbackError::AlwaysClosed("confidential".into())));
        let mut f = fallback();
        f.tools.insert("search".into(), "internal".into());
        assert_eq!(f.validate(), Err(FallbackError::BadTool("search".into())));
        let mut f = fallback();
        f.tools.insert("drive/x y".into(), "internal".into());
        assert!(f.validate().is_err());
        let mut f = fallback();
        f.servers.insert("drive".into(), "secret".into());
        assert_eq!(f.validate(), Err(FallbackError::UnknownTier("secret".into())));
    }

    #[test]
    fn the_signed_content_is_sorted_and_changes_with_any_entry() {
        let f = fallback();
        assert_eq!(
            f.content(),
            "server admin crown_jewels\ntool admin/reset_password crown_jewels\ntool drive/search internal\n\
             fail_open internal\n"
        );
        let mut g = fallback();
        g.tools.insert("drive/search".into(), "public".into());
        assert_ne!(f.content(), g.content());
    }
}

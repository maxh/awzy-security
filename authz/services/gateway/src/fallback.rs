//! The outage fallback setting (ADR 0089): each MCP tool's tier, which every check sends the PDP,
//! and which tiers fail open while the PDP is unreachable (ADR 0030). The client sets it in the
//! portal; awzy-policy keeps and signs it; the gateway follows it and verifies each version with
//! the key PDPs verify bundles with, so changing it in the database changes nothing here.
//!
//! Until a verified setting arrives every tool is unclassified (confidential) and every call fails
//! closed during an outage. A setting that doesn't verify, or is older than the one held, is
//! ignored and the held one stays.

use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use awzy_authz_proto::policy::v1::{
    policy_service_client::PolicyServiceClient, GetOutageFallbackRequest, OutageFallback as WireFallback,
};
use awzy_model::OutageFallback;
use awzy_service_auth::AuthChannel;
use awzy_signing::BundleVerifier;

/// How often the gateway asks awzy-policy for a newer setting.
const POLL: Duration = Duration::from_secs(10);

pub struct Fallback {
    /// The version held, 0 for none, and the setting.
    current: RwLock<(i64, OutageFallback)>,
}

impl Fallback {
    /// Nothing classified until a verified setting arrives.
    pub fn awaiting() -> Self {
        Self { current: RwLock::new((0, OutageFallback::default())) }
    }

    /// A fixed setting, never followed: for tests and the demo.
    pub fn fixed(fallback: OutageFallback) -> Self {
        Self { current: RwLock::new((1, fallback)) }
    }

    pub fn version(&self) -> i64 {
        self.current.read().expect("fallback lock").0
    }

    /// For a check: the tool's tier, its server's, or unclassified.
    pub fn tier(&self, server: &str, tool: &str) -> String {
        self.current.read().expect("fallback lock").1.tier(server, tool).to_string()
    }

    /// During an outage: the tool's tier if it's listed by name, and whether that tier fails open.
    pub fn outage_tier(&self, server: &str, tool: &str) -> Option<(String, bool)> {
        let current = self.current.read().expect("fallback lock");
        current.1.listed_tier(server, tool).map(|t| (t.to_string(), current.1.fails_open(t)))
    }

    /// Takes `wire` if it's newer than the setting held and verifies; returns whether it did.
    /// Without `verifier`, as when run locally, an unsigned setting is taken.
    pub fn accept(&self, wire: WireFallback, verifier: Option<&BundleVerifier>) -> Result<bool, String> {
        if wire.version <= self.version() {
            return Ok(false);
        }
        let fallback = OutageFallback {
            tools: wire.tools.into_iter().collect(),
            servers: wire.servers.into_iter().collect(),
            fail_open_tiers: wire.fail_open_tiers.into_iter().collect(),
        };
        fallback.validate().map_err(|e| e.to_string())?;
        if let Some(verifier) = verifier {
            let by =
                awzy_authz_proto::actor_from_wire(wire.changed_by_actor, "").map(|a| a.to_string()).unwrap_or_default();
            let at = wire.changed_at.map(|t| t.seconds).unwrap_or_default();
            let payload = awzy_signing::fallback_payload(wire.version, &by, at, &fallback.content());
            verifier.verify_payload(&payload, &wire.signature, &wire.signing_key_id).map_err(|e| e.to_string())?;
        }
        *self.current.write().expect("fallback lock") = (wire.version, fallback);
        Ok(true)
    }
}

/// Follows awzy-policy forever. Errors are logged and the held setting stays.
pub async fn follow(
    fallback: Arc<Fallback>,
    mut client: PolicyServiceClient<AuthChannel>,
    verifier: Option<Arc<BundleVerifier>>,
) {
    loop {
        let req = GetOutageFallbackRequest { known_version: fallback.version() };
        match client.get_outage_fallback(req).await {
            Ok(res) => {
                let res = res.into_inner();
                if let Some(wire) = res.fallback.filter(|_| !res.unchanged) {
                    let version = wire.version;
                    match fallback.accept(wire, verifier.as_deref()) {
                        Ok(true) => tracing::info!(version, "outage fallback setting loaded"),
                        Ok(false) => {}
                        Err(error) => {
                            tracing::error!(version, %error, "refused an outage fallback setting; keeping the one held")
                        }
                    }
                }
            }
            Err(status) => tracing::warn!(%status, "couldn't reach awzy-policy for the outage fallback setting"),
        }
        tokio::time::sleep(POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use p256::ecdsa::SigningKey;

    use super::*;

    fn signed(version: i64, fallback: &OutageFallback, key: u8) -> WireFallback {
        let signer = awzy_signing::BundleSigner::from_key(SigningKey::from_bytes(&[key; 32].into()).unwrap()).unwrap();
        let payload = awzy_signing::fallback_payload(version, "Human::alice", 1_800_000_000, &fallback.content());
        WireFallback {
            version,
            tools: fallback.tools.clone().into_iter().collect(),
            servers: fallback.servers.clone().into_iter().collect(),
            fail_open_tiers: fallback.fail_open_tiers.iter().cloned().collect(),
            changed_by_actor: awzy_authz_proto::actor_to_wire("Human::alice").0,
            changed_at: Some(awzy_authz_proto::prost_types::Timestamp { seconds: 1_800_000_000, nanos: 0 }),
            signature: signer.sign_payload(&payload),
            signing_key_id: signer.key_id().to_string(),
        }
    }

    fn drive_search_fails_open() -> OutageFallback {
        let mut f = OutageFallback::default();
        f.tools.insert("drive/search".into(), "internal".into());
        f.fail_open_tiers.insert("internal".into());
        f
    }

    #[test]
    fn only_a_newer_setting_that_verifies_is_used() {
        let verifier =
            BundleVerifier::from_keys([*SigningKey::from_bytes(&[7u8; 32].into()).unwrap().verifying_key()]).unwrap();
        let fallback = Fallback::awaiting();
        assert_eq!(fallback.tier("drive", "search"), "confidential");
        assert_eq!(fallback.outage_tier("drive", "search"), None);

        let f = drive_search_fails_open();
        assert_eq!(fallback.accept(signed(2, &f, 7), Some(&verifier)), Ok(true));
        assert_eq!(fallback.outage_tier("drive", "search"), Some(("internal".into(), true)));

        // Signed by another key, or changed after signing: refused, and the held one stays.
        let mut other = drive_search_fails_open();
        other.tools.insert("drive/delete_file".into(), "internal".into());
        assert!(fallback.accept(signed(3, &other, 9), Some(&verifier)).is_err());
        let mut tampered = signed(3, &f, 7);
        tampered.tools.insert("drive/delete_file".into(), "internal".into());
        assert!(fallback.accept(tampered, Some(&verifier)).is_err());
        assert_eq!(fallback.outage_tier("drive", "delete_file"), None);

        // An older genuine setting can't be replayed over a newer one.
        assert_eq!(fallback.accept(signed(1, &OutageFallback::default(), 7), Some(&verifier)), Ok(false));
        assert_eq!(fallback.version(), 2);
    }
}

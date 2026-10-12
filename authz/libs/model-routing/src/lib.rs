//! The model routing setting from awzy-policy (ADR 0114), taken by a service's [`ModelRouter`]:
//! the client portal for Awzy Bot, awzy-reasoner for investigations. A routing is used only when
//! it's newer than the one held, signed by the install's key, and names only the connections and
//! APIs this service has; otherwise the one held stays, and the refusal is logged.

use std::{
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
    time::Duration,
};

use awzy_authz_proto::policy::v1::{
    policy_service_client::PolicyServiceClient, GetModelRoutingRequest, ModelRouting as WireRouting,
};
use awzy_llm::{LlmError, ModelRouter, Routing};
use awzy_service_auth::AuthChannel;
use awzy_signing::{BundleVerifier, SigningError};

/// How often awzy-policy is asked for a newer routing.
const POLL: Duration = Duration::from_secs(10);

/// Why a routing from awzy-policy wasn't taken.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("its signature doesn't verify: {0}")]
    Signature(#[from] SigningError),
    #[error("it isn't a model routing: {0}")]
    Unreadable(#[from] serde_json::Error),
    #[error("this service can't use it: {0}")]
    Invalid(#[from] LlmError),
}

/// Why the key routings are checked with couldn't be loaded.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("reading {0}: {1}")]
    Read(String, std::io::Error),
    #[error(transparent)]
    Key(#[from] SigningError),
}

/// The keys awzy-policy signs with, from AWZY_POLICY_VERIFY_KEY_FILE, as the PDPs and gateways
/// read them. Unset, as when run locally: none, and unsigned routings are taken.
pub fn verifier_from_env() -> Result<Option<Arc<BundleVerifier>>, KeyError> {
    let Ok(path) = std::env::var("AWZY_POLICY_VERIFY_KEY_FILE") else {
        tracing::warn!("AWZY_POLICY_VERIFY_KEY_FILE is not set; model routings aren't verified");
        return Ok(None);
    };
    let pem = std::fs::read_to_string(&path).map_err(|e| KeyError::Read(path.clone(), e))?;
    Ok(Some(Arc::new(BundleVerifier::from_pem_bundle(&pem)?)))
}

/// A router and the version of the setting it holds.
pub struct RoutingSetting {
    router: Arc<ModelRouter>,
    /// 0 until a setting is taken: the router keeps the routing it started with.
    version: AtomicI64,
}

impl RoutingSetting {
    pub fn new(router: Arc<ModelRouter>) -> Self {
        Self { router, version: AtomicI64::new(0) }
    }

    pub fn version(&self) -> i64 {
        self.version.load(Ordering::Acquire)
    }

    /// Takes `wire` if it's newer than the setting held, verifies, and suits this service's
    /// connections; returns whether it did. Without `verifier`, as when run locally, an unsigned
    /// setting is taken.
    pub fn accept(&self, wire: WireRouting, verifier: Option<&BundleVerifier>) -> Result<bool, Refused> {
        if wire.version <= self.version() {
            return Ok(false);
        }
        if let Some(verifier) = verifier {
            let by =
                awzy_authz_proto::actor_from_wire(wire.changed_by_actor, "").map(|a| a.to_string()).unwrap_or_default();
            let at = wire.changed_at.map(|t| t.seconds).unwrap_or_default();
            let payload = awzy_signing::model_routing_payload(wire.version, &by, at, &wire.routing_json);
            verifier.verify_payload(&payload, &wire.signature, &wire.signing_key_id)?;
        }
        let routing: Routing = serde_json::from_str(&wire.routing_json)?;
        self.router.set_routing(routing)?;
        self.version.store(wire.version, Ordering::Release);
        Ok(true)
    }

    /// Follows awzy-policy's setting forever, asking every few seconds.
    pub async fn follow(self: Arc<Self>, mut client: PolicyServiceClient<AuthChannel>, verifier: Option<Arc<BundleVerifier>>) {
        loop {
            match client.get_model_routing(GetModelRoutingRequest { known_version: self.version() }).await {
                Ok(res) => {
                    let res = res.into_inner();
                    if let Some(wire) = res.routing.filter(|_| !res.unchanged) {
                        let version = wire.version;
                        match self.accept(wire, verifier.as_deref()) {
                            Ok(true) => tracing::info!(version, "model routing loaded"),
                            Ok(false) => {}
                            Err(error) => tracing::error!(version, %error, "refused a model routing; keeping the one held"),
                        }
                    }
                }
                Err(status) => tracing::warn!(%status, "couldn't reach awzy-policy for the model routing"),
            }
            tokio::time::sleep(POLL).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use awzy_llm::{Connection, Connections};
    use p256::ecdsa::SigningKey;
    use serde_json::json;

    use super::*;

    fn routing(model: &str) -> String {
        json!({
            "models": [{ "name": "m", "connection": "local", "spec": { "api": "openai_chat", "model": model } }],
            "routes": { "awzy_bot": { "targets": [{ "model": "m" }] } },
        })
        .to_string()
    }

    fn signed(version: i64, routing_json: &str, key: u8) -> WireRouting {
        let signer = awzy_signing::BundleSigner::from_key(SigningKey::from_bytes(&[key; 32].into()).unwrap()).unwrap();
        let payload = awzy_signing::model_routing_payload(version, "Human::alice", 1_800_000_000, routing_json);
        WireRouting {
            version,
            routing_json: routing_json.into(),
            changed_by_actor: awzy_authz_proto::actor_to_wire("Human::alice").0,
            changed_at: Some(awzy_authz_proto::prost_types::Timestamp { seconds: 1_800_000_000, nanos: 0 }),
            signature: signer.sign_payload(&payload),
            signing_key_id: signer.key_id().to_string(),
        }
    }

    async fn setting() -> RoutingSetting {
        let connections: Connections =
            [("local".to_string(), Connection::Openai { url: "http://vllm:8000/v1".into(), api_key_env: None })].into();
        RoutingSetting::new(Arc::new(ModelRouter::new(connections, Routing::default()).await.unwrap()))
    }

    #[tokio::test]
    async fn only_a_newer_routing_that_verifies_and_suits_the_service_is_used() {
        let verifier =
            BundleVerifier::from_keys([*SigningKey::from_bytes(&[7u8; 32].into()).unwrap().verifying_key()]).unwrap();
        let s = setting().await;
        assert!(s.accept(signed(2, &routing("llama"), 7), Some(&verifier)).unwrap());
        assert_eq!(s.router.conversation("awzy_bot").unwrap().model(), "m");

        // Signed by another key, or changed after signing: refused, and the held one stays.
        assert!(matches!(s.accept(signed(3, &routing("qwen"), 9), Some(&verifier)), Err(Refused::Signature(_))));
        let mut tampered = signed(3, &routing("qwen"), 7);
        tampered.routing_json = routing("mistral");
        assert!(s.accept(tampered, Some(&verifier)).is_err());
        assert_eq!(s.version(), 2);

        // A connection this service doesn't have is refused even when signed.
        let elsewhere = routing("llama").replace("\"local\"", "\"public-api\"");
        assert!(matches!(s.accept(signed(4, &elsewhere, 7), Some(&verifier)), Err(Refused::Invalid(_))));

        // An older genuine routing can't be replayed over a newer one.
        assert!(!s.accept(signed(1, &routing("qwen"), 7), Some(&verifier)).unwrap());
        assert_eq!(s.version(), 2);
    }
}

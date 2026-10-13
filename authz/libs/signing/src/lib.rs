//! Signing and verifying policy bundles: ECDSA P-256 with SHA-256, which every cloud KMS
//! supports (AWS KMS, Cloud KMS, Key Vault).
//!
//! awzy-policy signs a bundle when it is published; awzy-pdp verifies it before loading. A
//! bundle changed in the database after publishing no longer verifies, so tampering with the
//! policy store can't change what is enforced.
//!
//! The signed payload is text with one field per line, so it can't be ambiguous:
//!
//! ```text
//! awzy.policy.v1
//! <bundle id QID>
//! <version>
//! <sha256 of schema, hex>
//! <sha256 of policies, hex>
//! ```
//!
//! awzy-policy signs enforcement mode changes the same way ([`enforcement_payload`]), since
//! switching an install to monitor changes what's enforced as surely as a bundle does.
//!
//! And outage fallback settings ([`fallback_payload`]), since which tools fail open during a PDP
//! outage decides what's allowed then.
//!
//! The same keys sign break-glass requests (see [`break_glass_payload`]), which admins enrolled
//! at install time send with the `awzy-break-glass` tool.
//!
//! An install signs with its cloud's KMS, so the private key never leaves it (ADR 0092): a
//! [`PayloadSigner`] like `awzy_signing_aws::KmsSigner`. [`BundleSigner`] signs with a PEM key,
//! for local runs and tests. Either way the signature is DER and the key ID is the public key's
//! fingerprint ([`key_id_of`]), so verifiers can't tell them apart.

use p256::{
    ecdsa::{
        signature::{Signer, Verifier},
        Signature, SigningKey, VerifyingKey,
    },
    pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePublicKey},
};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("invalid key: {0}")]
    Key(String),
    #[error("not signed")]
    Unsigned,
    #[error("signed with key {0:?}, which isn't trusted here")]
    UnknownKey(String),
    #[error("signature doesn't verify: the content changed after signing, or another key signed it")]
    BadSignature,
    #[error("signing failed: {0}")]
    Signer(String),
}

/// The bytes that get signed.
pub fn payload(id: &str, version: i64, schema: &str, policies: &str) -> Vec<u8> {
    let hex = |s: &str| format!("{:x}", Sha256::digest(s.as_bytes()));
    format!("awzy.policy.v1\n{id}\n{version}\n{}\n{}\n", hex(schema), hex(policies)).into_bytes()
}

/// The bytes a break-glass rollback request signs. `to_version` 0 means "the bundle before the
/// active one".
pub fn break_glass_payload(to_version: i64, requested_at_unix: i64, nonce: &str, reason: &str) -> Vec<u8> {
    let reason = format!("{:x}", Sha256::digest(reason.as_bytes()));
    format!("awzy.break-glass.v1\nrollback\n{to_version}\n{requested_at_unix}\n{nonce}\n{reason}\n").into_bytes()
}

/// The bytes an enforcement mode change signs (ADR 0080): the scope, the mode, who set it, and
/// when, in Unix seconds. The time lets a PDP refuse an older change offered as the current one.
pub fn enforcement_payload(scope: &str, mode: &str, changed_by: &str, changed_at_unix: i64) -> Vec<u8> {
    format!("awzy.enforcement.v1\n{scope}\n{mode}\n{changed_by}\n{changed_at_unix}\n").into_bytes()
}

/// The bytes an outage fallback setting signs (ADR 0089): its version, who set it and when, in
/// Unix seconds, and the SHA-256 of its content (`OutageFallback::content`). The version lets a
/// gateway refuse an older setting offered as the current one.
pub fn fallback_payload(version: i64, changed_by: &str, changed_at_unix: i64, content: &str) -> Vec<u8> {
    let content = format!("{:x}", Sha256::digest(content.as_bytes()));
    format!("awzy.fallback.v1\n{version}\n{changed_by}\n{changed_at_unix}\n{content}\n").into_bytes()
}

/// The bytes a model routing setting signs (ADR 0120): its version, who set it and when, in Unix
/// seconds, and the SHA-256 of its JSON as stored. The version lets a service refuse an older
/// routing offered as the current one.
pub fn model_routing_payload(version: i64, changed_by: &str, changed_at_unix: i64, routing_json: &str) -> Vec<u8> {
    let content = format!("{:x}", Sha256::digest(routing_json.as_bytes()));
    format!("awzy.model-routing.v1\n{version}\n{changed_by}\n{changed_at_unix}\n{content}\n").into_bytes()
}

/// Signs payloads: a PEM key ([`BundleSigner`]) or a cloud KMS key.
#[async_trait::async_trait]
pub trait PayloadSigner: Send + Sync {
    /// `sha256:<fingerprint>` of the public key, as [`key_id_of`] computes it.
    fn key_id(&self) -> &str;

    /// A DER-encoded ECDSA P-256 signature over `payload`, hashed with SHA-256.
    async fn sign_payload(&self, payload: &[u8]) -> Result<Vec<u8>, SigningError>;
}

/// `sha256:<first 16 hex>` of the public key's DER encoding: the key ID that travels with each
/// signature.
pub fn key_id_of(key: &VerifyingKey) -> Result<String, SigningError> {
    let der = key.to_public_key_der().map_err(|e| SigningError::Key(e.to_string()))?;
    Ok(format!("sha256:{}", &format!("{:x}", Sha256::digest(der.as_bytes()))[..16]))
}

pub struct BundleSigner {
    key: SigningKey,
    key_id: String,
}

impl BundleSigner {
    /// From a PKCS#8 PEM private key, e.g. `openssl ecparam -name prime256v1 -genkey -noout |
    /// openssl pkcs8 -topk8 -nocrypt`.
    pub fn from_pem(pem: &str) -> Result<Self, SigningError> {
        let key = SigningKey::from_pkcs8_pem(pem).map_err(|e| SigningError::Key(e.to_string()))?;
        Self::from_key(key)
    }

    pub fn from_key(key: SigningKey) -> Result<Self, SigningError> {
        let key_id = key_id_of(key.verifying_key())?;
        Ok(Self { key, key_id })
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// DER-encoded signature over a bundle's payload.
    pub fn sign(&self, id: &str, version: i64, schema: &str, policies: &str) -> Vec<u8> {
        self.sign_payload(&payload(id, version, schema, policies))
    }

    /// DER-encoded signature over any payload, e.g. [`break_glass_payload`].
    pub fn sign_payload(&self, payload: &[u8]) -> Vec<u8> {
        let sig: Signature = self.key.sign(payload);
        sig.to_der().as_bytes().to_vec()
    }
}

#[async_trait::async_trait]
impl PayloadSigner for BundleSigner {
    fn key_id(&self) -> &str {
        &self.key_id
    }

    async fn sign_payload(&self, payload: &[u8]) -> Result<Vec<u8>, SigningError> {
        Ok(BundleSigner::sign_payload(self, payload))
    }
}

/// The public keys a PDP trusts, by key ID.
pub struct BundleVerifier {
    keys: Vec<(String, VerifyingKey)>,
}

impl BundleVerifier {
    /// From one SPKI PEM public key, e.g. `openssl ec -pubout`.
    pub fn from_pem(pem: &str) -> Result<Self, SigningError> {
        let key = VerifyingKey::from_public_key_pem(pem).map_err(|e| SigningError::Key(e.to_string()))?;
        Self::from_keys([key])
    }

    /// From one or more concatenated SPKI PEM public keys, e.g. the enrolled break-glass admins.
    pub fn from_pem_bundle(text: &str) -> Result<Self, SigningError> {
        const BEGIN: &str = "-----BEGIN PUBLIC KEY-----";
        let keys = text
            .split(BEGIN)
            .filter(|block| block.contains("-----END PUBLIC KEY-----"))
            .map(|block| {
                VerifyingKey::from_public_key_pem(&format!("{BEGIN}{block}"))
                    .map_err(|e| SigningError::Key(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if keys.is_empty() {
            return Err(SigningError::Key("no public keys found".into()));
        }
        Self::from_keys(keys)
    }

    pub fn key_ids(&self) -> impl Iterator<Item = &str> {
        self.keys.iter().map(|(id, _)| id.as_str())
    }

    pub fn from_keys(keys: impl IntoIterator<Item = VerifyingKey>) -> Result<Self, SigningError> {
        Ok(Self { keys: keys.into_iter().map(|k| Ok((key_id_of(&k)?, k))).collect::<Result<_, SigningError>>()? })
    }

    pub fn verify(
        &self,
        id: &str,
        version: i64,
        schema: &str,
        policies: &str,
        signature: &[u8],
        key_id: &str,
    ) -> Result<(), SigningError> {
        self.verify_payload(&payload(id, version, schema, policies), signature, key_id)
    }

    /// Verifies a signature over any payload, e.g. [`break_glass_payload`].
    pub fn verify_payload(&self, payload: &[u8], signature: &[u8], key_id: &str) -> Result<(), SigningError> {
        if signature.is_empty() {
            return Err(SigningError::Unsigned);
        }
        let (_, key) =
            self.keys.iter().find(|(k, _)| k == key_id).ok_or_else(|| SigningError::UnknownKey(key_id.to_string()))?;
        let sig = Signature::from_der(signature).map_err(|_| SigningError::BadSignature)?;
        key.verify(payload, &sig).map_err(|_| SigningError::BadSignature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32].into()).unwrap()
    }

    #[test]
    fn signs_and_verifies() {
        let signer = BundleSigner::from_key(key(7)).unwrap();
        let verifier = BundleVerifier::from_keys([*key(7).verifying_key()]).unwrap();
        let sig = signer.sign("policy_bundle_1", 3, "schema", "policies");
        assert!(signer.key_id().starts_with("sha256:"));
        verifier.verify("policy_bundle_1", 3, "schema", "policies", &sig, signer.key_id()).unwrap();
    }

    #[test]
    fn rejects_tampering_unknown_keys_and_unsigned_bundles() {
        let signer = BundleSigner::from_key(key(7)).unwrap();
        let verifier = BundleVerifier::from_keys([*key(7).verifying_key()]).unwrap();
        let sig = signer.sign("policy_bundle_1", 3, "schema", "policies");
        let id = signer.key_id();
        for (bundle, version, schema, policies) in [
            ("policy_bundle_1", 3, "schema", "permit (principal, action, resource);"),
            ("policy_bundle_1", 4, "schema", "policies"),
            ("policy_bundle_2", 3, "schema", "policies"),
            ("policy_bundle_1", 3, "changed", "policies"),
        ] {
            assert!(matches!(
                verifier.verify(bundle, version, schema, policies, &sig, id),
                Err(SigningError::BadSignature)
            ));
        }
        let other = BundleSigner::from_key(key(9)).unwrap();
        let sig2 = other.sign("policy_bundle_1", 3, "schema", "policies");
        assert!(matches!(
            verifier.verify("policy_bundle_1", 3, "schema", "policies", &sig2, other.key_id()),
            Err(SigningError::UnknownKey(_))
        ));
        assert!(matches!(
            verifier.verify("policy_bundle_1", 3, "schema", "policies", &[], id),
            Err(SigningError::Unsigned)
        ));
    }

    #[test]
    fn verifies_any_enrolled_key_from_a_pem_bundle() {
        use p256::pkcs8::{EncodePublicKey, LineEnding};
        let pem = |seed: u8| key(seed).verifying_key().to_public_key_pem(LineEnding::LF).unwrap();
        let verifier = BundleVerifier::from_pem_bundle(&format!("{}{}", pem(1), pem(2))).unwrap();
        assert_eq!(verifier.key_ids().count(), 2);
        let admin = BundleSigner::from_key(key(2)).unwrap();
        let req = break_glass_payload(0, 1_700_000_000, "n1", "bad bundle locked us out");
        verifier.verify_payload(&req, &admin.sign_payload(&req), admin.key_id()).unwrap();
        let other_reason = break_glass_payload(0, 1_700_000_000, "n1", "something else");
        assert!(verifier.verify_payload(&other_reason, &admin.sign_payload(&req), admin.key_id()).is_err());
    }

    #[test]
    fn payload_is_unambiguous() {
        // Moving text between fields changes the payload.
        assert_ne!(payload("a", 1, "x\ny", "z"), payload("a", 1, "x", "y\nz"));
    }

    #[test]
    fn an_enforcement_change_verifies_only_as_signed() {
        let signer = BundleSigner::from_key(key(7)).unwrap();
        let verifier = BundleVerifier::from_keys([*key(7).verifying_key()]).unwrap();
        let sig = signer.sign_payload(&enforcement_payload("install", "enforce", "Human::alice", 1_800_000_000));
        let id = signer.key_id();
        assert!(verifier
            .verify_payload(&enforcement_payload("install", "enforce", "Human::alice", 1_800_000_000), &sig, id)
            .is_ok());
        for changed in [
            enforcement_payload("install", "monitor", "Human::alice", 1_800_000_000),
            enforcement_payload("automated-containments", "enforce", "Human::alice", 1_800_000_000),
            enforcement_payload("install", "enforce", "Human::mallory", 1_800_000_000),
            enforcement_payload("install", "enforce", "Human::alice", 1_800_000_001),
        ] {
            assert!(matches!(verifier.verify_payload(&changed, &sig, id), Err(SigningError::BadSignature)));
        }
    }

    #[test]
    fn a_fallback_setting_verifies_only_as_signed() {
        let signer = BundleSigner::from_key(key(7)).unwrap();
        let verifier = BundleVerifier::from_keys([*key(7).verifying_key()]).unwrap();
        let content = "tool drive/search internal\nfail_open internal\n";
        let sig = signer.sign_payload(&fallback_payload(3, "Human::alice", 1_800_000_000, content));
        let id = signer.key_id();
        assert!(verifier
            .verify_payload(&fallback_payload(3, "Human::alice", 1_800_000_000, content), &sig, id)
            .is_ok());
        for changed in [
            fallback_payload(2, "Human::alice", 1_800_000_000, content),
            fallback_payload(3, "Human::mallory", 1_800_000_000, content),
            fallback_payload(3, "Human::alice", 1_800_000_001, content),
            fallback_payload(3, "Human::alice", 1_800_000_000, "tool drive/search internal\nfail_open public\n"),
        ] {
            assert!(matches!(verifier.verify_payload(&changed, &sig, id), Err(SigningError::BadSignature)));
        }
    }
}

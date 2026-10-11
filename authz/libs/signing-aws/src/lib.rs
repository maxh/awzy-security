//! Signing with an AWS KMS key (`ECC_NIST_P256`, `ECDSA_SHA_256`), so the private key never
//! leaves KMS (ADR 0092). KMS returns DER signatures, and the key ID is the fingerprint of the
//! public key KMS gives, so PDPs verify these exactly as they verify a PEM key's.

use aws_config::BehaviorVersion;
use aws_sdk_kms::{
    primitives::Blob,
    types::{KeySpec, MessageType, SigningAlgorithmSpec},
    Client,
};
use aws_smithy_http_client::tls;
use awzy_signing::{key_id_of, PayloadSigner, SigningError};
use p256::{ecdsa::VerifyingKey, pkcs8::DecodePublicKey};

pub struct KmsSigner {
    client: Client,
    key: String,
    key_id: String,
}

impl KmsSigner {
    /// For the key `key` (its ARN), with credentials and region from the environment: in an
    /// install, awzy-policy's Pod Identity. Fails unless the key is P-256 and these credentials
    /// may read its public key.
    pub async fn connect(key: &str) -> Result<Self, SigningError> {
        let https = aws_smithy_http_client::Builder::new()
            .tls_provider(tls::Provider::Rustls(tls::rustls_provider::CryptoMode::Ring))
            .build_https();
        let aws = aws_config::defaults(BehaviorVersion::latest()).http_client(https).load().await;
        let client = Client::new(&aws);
        let public = client
            .get_public_key()
            .key_id(key)
            .send()
            .await
            .map_err(|e| SigningError::Signer(format!("reading {key}'s public key: {}", e.into_service_error())))?;
        let der = public.public_key().ok_or_else(|| SigningError::Signer(format!("{key} has no public key")))?;
        let key_id = key_id_from_kms(public.key_spec(), der.as_ref())?;
        Ok(Self { client, key: key.to_string(), key_id })
    }
}

/// The key ID of a KMS key, from what GetPublicKey says: its spec, which must be P-256, and its
/// public key (DER SPKI).
fn key_id_from_kms(spec: Option<&KeySpec>, der: &[u8]) -> Result<String, SigningError> {
    if spec != Some(&KeySpec::EccNistP256) {
        return Err(SigningError::Key(format!("the KMS key is {spec:?}, not ECC_NIST_P256")));
    }
    let key = VerifyingKey::from_public_key_der(der).map_err(|e| SigningError::Key(e.to_string()))?;
    key_id_of(&key)
}

#[async_trait::async_trait]
impl PayloadSigner for KmsSigner {
    fn key_id(&self) -> &str {
        &self.key_id
    }

    async fn sign_payload(&self, payload: &[u8]) -> Result<Vec<u8>, SigningError> {
        // KMS hashes a RAW message itself, up to 4 KiB; awzy's payloads are a few lines.
        let signed = self
            .client
            .sign()
            .key_id(&self.key)
            .message(Blob::new(payload))
            .message_type(MessageType::Raw)
            .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256)
            .send()
            .await
            .map_err(|e| SigningError::Signer(e.into_service_error().to_string()))?;
        signed
            .signature()
            .map(|s| s.as_ref().to_vec())
            .ok_or_else(|| SigningError::Signer("KMS returned no signature".into()))
    }
}

#[cfg(test)]
mod tests {
    use p256::{ecdsa::SigningKey, pkcs8::EncodePublicKey};

    use super::*;

    #[test]
    fn a_kms_key_has_the_key_id_its_pem_would() {
        let key = SigningKey::from_bytes(&[5u8; 32].into()).unwrap();
        let der = key.verifying_key().to_public_key_der().unwrap();
        let pem_signer = awzy_signing::BundleSigner::from_key(key).unwrap();
        assert_eq!(key_id_from_kms(Some(&KeySpec::EccNistP256), der.as_bytes()).unwrap(), pem_signer.key_id());
        assert!(key_id_from_kms(Some(&KeySpec::Rsa2048), der.as_bytes()).is_err());
    }
}

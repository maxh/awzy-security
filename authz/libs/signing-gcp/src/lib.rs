//! Signing with a Cloud KMS key (`EC_SIGN_P256_SHA256`), so the private key never leaves KMS (ADR
//! 0092): the Google Cloud counterpart of `awzy_signing_aws`. KMS returns DER signatures, and the
//! key ID is the fingerprint of the public key KMS gives, so PDPs verify these exactly as they
//! verify a PEM key's.
//!
//! Plain HTTPS calls to KMS's REST API, with an access token from the GKE metadata server (the
//! pod's Workload Identity), rather than Google's client libraries: two calls.

use std::time::{Duration, Instant};

use awzy_signing::{key_id_of, PayloadSigner, SigningError};
use base64::{engine::general_purpose::STANDARD, Engine};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use p256::{ecdsa::VerifyingKey, pkcs8::DecodePublicKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

const METADATA: &str = "http://metadata.google.internal";
const KMS: &str = "https://cloudkms.googleapis.com";
const TIMEOUT: Duration = Duration::from_secs(10);

type HttpClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>;

/// Whether `key` names a Cloud KMS key version
/// (`projects/…/locations/…/keyRings/…/cryptoKeys/…/cryptoKeyVersions/…`), rather than another
/// cloud's key.
pub fn is_key_name(key: &str) -> bool {
    key.starts_with("projects/") && key.contains("/cryptoKeyVersions/")
}

pub struct KmsSigner {
    http: HttpClient,
    metadata: String,
    kms: String,
    key: String,
    key_id: String,
    token: Mutex<Option<(String, Instant)>>,
}

impl KmsSigner {
    /// For the key version `key`, with the pod's Workload Identity. Fails unless the key is P-256
    /// and that identity may read its public key.
    pub async fn connect(key: &str) -> Result<Self, SigningError> {
        Self::connect_to(key, METADATA, KMS).await
    }

    /// Against other metadata and KMS endpoints: for tests.
    pub async fn connect_to(key: &str, metadata: &str, kms: &str) -> Result<Self, SigningError> {
        if !is_key_name(key) {
            return Err(SigningError::Key(format!("{key:?} isn't a Cloud KMS key version")));
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let https =
            hyper_rustls::HttpsConnectorBuilder::new().with_webpki_roots().https_or_http().enable_http1().build();
        let mut signer = Self {
            http: Client::builder(TokioExecutor::new()).build(https),
            metadata: metadata.trim_end_matches('/').to_string(),
            kms: kms.trim_end_matches('/').to_string(),
            key: key.to_string(),
            key_id: String::new(),
            token: Mutex::new(None),
        };
        let public = signer.call("GET", &format!("v1/{key}/publicKey"), None).await?;
        if public["algorithm"] != "EC_SIGN_P256_SHA256" {
            return Err(SigningError::Key(format!("{key} is {}, not EC_SIGN_P256_SHA256", public["algorithm"])));
        }
        let pem = public["pem"].as_str().ok_or_else(|| SigningError::Signer(format!("{key} has no public key")))?;
        signer.key_id = key_id_from_pem(pem)?;
        Ok(signer)
    }

    /// An access token from the metadata server, kept until a minute before it expires.
    async fn token(&self) -> Result<String, SigningError> {
        let mut cached = self.token.lock().await;
        if let Some((token, until)) = cached.as_ref() {
            if Instant::now() < *until {
                return Ok(token.clone());
            }
        }
        let url = format!("{}/computeMetadata/v1/instance/service-accounts/default/token", self.metadata);
        let req = hyper::Request::get(&url)
            .header("metadata-flavor", "Google")
            .body(Full::new(Bytes::new()))
            .map_err(|e| SigningError::Signer(e.to_string()))?;
        let body = self.send(req, "the metadata server").await?;
        let token = body["access_token"]
            .as_str()
            .ok_or_else(|| SigningError::Signer("the metadata server gave no access token".into()))?
            .to_string();
        let lifetime = Duration::from_secs(body["expires_in"].as_u64().unwrap_or(300).saturating_sub(60));
        *cached = Some((token.clone(), Instant::now() + lifetime));
        Ok(token)
    }

    async fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value, SigningError> {
        let token = self.token().await?;
        let req = hyper::Request::builder()
            .method(method)
            .uri(format!("{}/{path}", self.kms))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(body.map(|b| b.to_string()).unwrap_or_default())))
            .map_err(|e| SigningError::Signer(e.to_string()))?;
        self.send(req, "Cloud KMS").await
    }

    async fn send(&self, req: hyper::Request<Full<Bytes>>, what: &str) -> Result<Value, SigningError> {
        let res = tokio::time::timeout(TIMEOUT, self.http.request(req))
            .await
            .map_err(|_| SigningError::Signer(format!("{what} timed out")))?
            .map_err(|e| SigningError::Signer(format!("{what}: {e}")))?;
        let status = res.status();
        let bytes =
            res.into_body().collect().await.map_err(|e| SigningError::Signer(format!("{what}: {e}")))?.to_bytes();
        if !status.is_success() {
            return Err(SigningError::Signer(format!("{what}: {status}: {}", String::from_utf8_lossy(&bytes))));
        }
        serde_json::from_slice(&bytes).map_err(|e| SigningError::Signer(format!("{what}: {e}")))
    }
}

/// The key ID of a KMS key from its public key, as PEM.
fn key_id_from_pem(pem: &str) -> Result<String, SigningError> {
    let key = VerifyingKey::from_public_key_pem(pem).map_err(|e| SigningError::Key(e.to_string()))?;
    key_id_of(&key)
}

#[async_trait::async_trait]
impl PayloadSigner for KmsSigner {
    fn key_id(&self) -> &str {
        &self.key_id
    }

    async fn sign_payload(&self, payload: &[u8]) -> Result<Vec<u8>, SigningError> {
        // KMS signs a digest; it hashes nothing itself for EC keys.
        let digest = STANDARD.encode(Sha256::digest(payload));
        let signed = self
            .call("POST", &format!("v1/{}:asymmetricSign", self.key), Some(json!({ "digest": { "sha256": digest } })))
            .await?;
        let signature =
            signed["signature"].as_str().ok_or_else(|| SigningError::Signer("KMS returned no signature".into()))?;
        STANDARD.decode(signature).map_err(|e| SigningError::Signer(format!("KMS's signature: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use axum::{extract::Path, http::HeaderMap, routing::get, routing::post, Json, Router};
    use p256::{
        ecdsa::{
            signature::{hazmat::PrehashSigner, Verifier},
            Signature, SigningKey,
        },
        pkcs8::{EncodePublicKey, LineEnding},
    };

    use super::*;

    const KEY: &str = "projects/p/locations/us-central1/keyRings/awzy/cryptoKeys/policy/cryptoKeyVersions/1";

    /// A metadata server and a Cloud KMS that sign with `key`, counting token requests.
    async fn fake(key: SigningKey, tokens: Arc<AtomicUsize>) -> String {
        let pem = key.verifying_key().to_public_key_pem(LineEnding::LF).unwrap();
        let app = Router::new()
            .route(
                "/computeMetadata/v1/instance/service-accounts/default/token",
                get(move |headers: HeaderMap| {
                    let tokens = tokens.clone();
                    async move {
                        assert_eq!(headers["metadata-flavor"], "Google");
                        tokens.fetch_add(1, Ordering::SeqCst);
                        Json(json!({ "access_token": "t", "expires_in": 3599 }))
                    }
                }),
            )
            .route(
                "/v1/{*rest}",
                get(move |Path(rest): Path<String>, headers: HeaderMap| {
                    let pem = pem.clone();
                    async move {
                        assert_eq!(headers["authorization"], "Bearer t");
                        assert_eq!(rest, format!("{KEY}/publicKey"));
                        Json(json!({ "pem": pem, "algorithm": "EC_SIGN_P256_SHA256" }))
                    }
                })
                .merge(post(move |Path(rest): Path<String>, Json(body): Json<Value>| async move {
                    assert_eq!(rest, format!("{KEY}:asymmetricSign"));
                    let digest = STANDARD.decode(body["digest"]["sha256"].as_str().unwrap()).unwrap();
                    let signature: Signature = key.sign_prehash(&digest).unwrap();
                    Json(json!({ "signature": STANDARD.encode(signature.to_der().as_bytes()) }))
                })),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn signs_what_a_pdp_verifies_with_the_key_id_its_pem_would_have() {
        let key = SigningKey::from_bytes(&[7u8; 32].into()).unwrap();
        let pem_signer = awzy_signing::BundleSigner::from_key(key.clone()).unwrap();
        let tokens = Arc::new(AtomicUsize::new(0));
        let base = fake(key.clone(), tokens.clone()).await;
        let signer = KmsSigner::connect_to(KEY, &base, &base).await.unwrap();
        assert_eq!(signer.key_id(), pem_signer.key_id());

        let payload = b"awzy policy bundle 7";
        let der = signer.sign_payload(payload).await.unwrap();
        let signature = Signature::from_der(&der).unwrap();
        assert!(key.verifying_key().verify(payload, &signature).is_ok());

        signer.sign_payload(b"another").await.unwrap();
        assert_eq!(tokens.load(Ordering::SeqCst), 1, "the token is kept until it nears expiry");
    }

    #[tokio::test]
    async fn only_a_cloud_kms_key_version_is_taken() {
        assert!(is_key_name(KEY));
        assert!(!is_key_name("arn:aws:kms:us-east-2:123456789012:key/abc"));
        assert!(KmsSigner::connect_to("arn:aws:kms:us-east-2:1:key/abc", "http://x", "http://x").await.is_err());
    }
}

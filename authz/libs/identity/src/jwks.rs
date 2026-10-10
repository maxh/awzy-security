//! The IdP's signing keys, from its JWKS endpoint. Fetched at first use and again when a token
//! names a key ID we don't have (the IdP rotated keys), at most every 30 seconds.
//!
//! HTTPS only, verified against the webpki roots plus an optional private CA (the local dev IdP
//! uses an mkcert or self-signed certificate this way). Keys fetched over plain HTTP could be
//! swapped in transit and used to forge tokens, so there is no exception, not even for localhost.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use jsonwebtoken::{jwk::JwkSet, DecodingKey};
use rustls_pki_types::{pem::PemObject, CertificateDer};
use tokio::sync::Mutex;

const MIN_REFRESH: Duration = Duration::from_secs(30);
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Jwks {
    uri: hyper::Uri,
    /// A file holding a bearer token to send with each fetch, read fresh every time: the
    /// Kubernetes API server serves its keys only to authenticated callers, and a pod's token
    /// rotates (ADR 0055).
    bearer_file: Option<std::path::PathBuf>,
    client: Client<hyper_rustls::HttpsConnector<HttpConnector>, Empty<Bytes>>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Option<Instant>,
}

impl Jwks {
    /// `extra_ca_pem`: PEM certificates to trust besides the webpki roots, for an IdP behind a
    /// private CA.
    pub fn new(url: &str, extra_ca_pem: Option<&[u8]>) -> anyhow::Result<Self> {
        let uri: hyper::Uri = url.parse()?;
        anyhow::ensure!(uri.scheme_str() == Some("https"), "JWKS URL must be https://: {url}");

        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        if let Some(pem) = extra_ca_pem {
            let certs = CertificateDer::pem_slice_iter(pem).collect::<Result<Vec<_>, _>>()?;
            anyhow::ensure!(!certs.is_empty(), "the CA file has no certificates");
            for cert in certs {
                roots.add(cert)?;
            }
        }
        let tls = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
        let https = hyper_rustls::HttpsConnectorBuilder::new().with_tls_config(tls).https_only().enable_http1().build();
        Ok(Self {
            bearer_file: None,
            uri,
            client: Client::builder(TokioExecutor::new()).build(https),
            state: Mutex::new(State::default()),
        })
    }

    /// Sends the token in `path` with each fetch.
    pub fn with_bearer_file(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.bearer_file = Some(path.into());
        self
    }

    /// The key with this ID, refreshing the set if it's unknown and the last fetch is old enough.
    pub async fn key(&self, kid: &str) -> anyhow::Result<DecodingKey> {
        let mut state = self.state.lock().await;
        if let Some(k) = state.keys.get(kid) {
            return Ok(k.clone());
        }
        if state.fetched_at.is_some_and(|t| t.elapsed() < MIN_REFRESH) {
            anyhow::bail!("no key {kid:?} in the IdP's JWKS");
        }
        // Count a failed fetch too, so an unreachable IdP isn't retried on every request.
        state.fetched_at = Some(Instant::now());
        state.keys = self.fetch().await?;
        tracing::info!(keys = state.keys.len(), url = %self.uri, "fetched IdP signing keys");
        state.keys.get(kid).cloned().ok_or_else(|| anyhow::anyhow!("no key {kid:?} in the IdP's JWKS"))
    }

    async fn fetch(&self) -> anyhow::Result<HashMap<String, DecodingKey>> {
        let mut req = hyper::Request::get(self.uri.clone());
        if let Some(path) = &self.bearer_file {
            let token = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("reading the bearer token in {}: {e}", path.display()))?;
            req = req.header(hyper::header::AUTHORIZATION, format!("Bearer {}", token.trim()));
        }
        let res = tokio::time::timeout(FETCH_TIMEOUT, self.client.request(req.body(Empty::new())?))
            .await
            .map_err(|_| anyhow::anyhow!("JWKS fetch from {} timed out", self.uri))??;
        anyhow::ensure!(res.status().is_success(), "JWKS fetch from {} returned {}", self.uri, res.status());
        let body = tokio::time::timeout(FETCH_TIMEOUT, res.into_body().collect())
            .await
            .map_err(|_| anyhow::anyhow!("JWKS fetch from {} timed out", self.uri))??
            .to_bytes();
        let set: JwkSet = serde_json::from_slice(&body)?;
        let mut keys = HashMap::new();
        for jwk in &set.keys {
            if let Some(kid) = &jwk.common.key_id {
                match DecodingKey::from_jwk(jwk) {
                    Ok(key) => {
                        keys.insert(kid.clone(), key);
                    }
                    Err(e) => tracing::warn!(%kid, error = %e, "skipping JWKS key"),
                }
            }
        }
        Ok(keys)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use axum::{routing::get, Json, Router};
    use hyper_util::{rt::TokioIo, service::TowerToHyperService};
    use serde_json::json;

    use super::*;

    #[test]
    fn only_https() {
        assert!(Jwks::new("https://idp.example.com/jwks", None).is_ok());
        assert!(Jwks::new("https://127.0.0.1:5556/jwks", None).is_ok());
        for url in [
            "http://idp.example.com/jwks",
            "http://127.0.0.1:5556/jwks",
            "http://localhost/jwks",
            "ftp://idp.example.com/jwks",
        ] {
            let err = Jwks::new(url, None).err().unwrap();
            assert!(err.to_string().contains("must be https"), "{url}: {err}");
        }
    }

    /// Serves `app` over HTTPS on localhost with a self-signed certificate. Returns the base URL
    /// and the certificate as PEM, to trust as a private CA.
    pub(crate) async fn serve_https(app: Router) -> (String, String) {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_pem = cert.cert.pem();
        let _ = rustls::crypto::ring::default_provider().install_default();
        let tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.cert.der().clone()],
                rustls_pki_types::PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into()),
            )
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let (tcp, _) = listener.accept().await.unwrap();
                let (acceptor, app) = (acceptor.clone(), app.clone());
                tokio::spawn(async move {
                    let Ok(tls) = acceptor.accept(tcp).await else { return };
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(tls), TowerToHyperService::new(app))
                        .await;
                });
            }
        });
        (format!("https://localhost:{port}"), cert_pem)
    }

    #[tokio::test]
    async fn fetches_keys_over_https_and_verifies_the_certificate() {
        let app = Router::new().route(
            "/jwks",
            get(|| async { Json(json!({ "keys": [{ "kty": "oct", "kid": "k1", "alg": "HS256", "k": "c2VjcmV0" }] })) }),
        );
        let (base, ca) = serve_https(app).await;
        let url = format!("{base}/jwks");

        let trusted = Jwks::new(&url, Some(ca.as_bytes())).unwrap();
        assert!(trusted.key("k1").await.is_ok(), "fetched over TLS with the private CA trusted");

        let untrusted = Jwks::new(&url, None).unwrap();
        let err = untrusted.key("k1").await.err().unwrap();
        let chain = format!("{err:#}");
        assert!(chain.contains("certificate"), "a self-signed certificate must not be accepted: {chain}");
    }
}

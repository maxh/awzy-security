//! HTTPS for the dev IdP. Real IdPs are HTTPS-only, and awzy fetches signing keys only over
//! HTTPS, so the dev IdP is too.
//!
//! The certificate comes from, in order:
//! 1. AWZY_DEV_IDP_TLS_CERT and AWZY_DEV_IDP_TLS_KEY (PEM files);
//! 2. `.dev-certs/localhost.pem` and `.dev-certs/localhost-key.pem` in the repository, made with
//!    mkcert (see DEVELOPMENT.md), which browsers trust after `mkcert -install`;
//! 3. a self-signed certificate made at startup, for tests and CI. Its PEM is written to
//!    AWZY_DEV_IDP_CA_OUT if set, so the portal and gateway can trust it (AWZY_AUTH_CA_FILE).

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Context;
use axum::Router;
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use rustls_pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use tokio::net::TcpListener;

pub struct TlsFiles {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    /// Where it came from, for the startup log.
    pub source: String,
}

/// `.dev-certs/` at the root of the checkout: BUILD_WORKSPACE_DIRECTORY under `bazel run`,
/// otherwise the current directory.
fn dev_certs_dir() -> PathBuf {
    std::env::var_os("BUILD_WORKSPACE_DIRECTORY").map(PathBuf::from).unwrap_or_default().join(".dev-certs")
}

pub fn load_or_generate() -> anyhow::Result<TlsFiles> {
    if let (Ok(cert), Ok(key)) = (std::env::var("AWZY_DEV_IDP_TLS_CERT"), std::env::var("AWZY_DEV_IDP_TLS_KEY")) {
        return Ok(TlsFiles {
            cert_pem: std::fs::read(&cert).with_context(|| format!("reading {cert}"))?,
            key_pem: std::fs::read(&key).with_context(|| format!("reading {key}"))?,
            source: cert,
        });
    }
    let dir = dev_certs_dir();
    let (cert, key) = (dir.join("localhost.pem"), dir.join("localhost-key.pem"));
    if cert.exists() && key.exists() {
        return Ok(TlsFiles {
            cert_pem: std::fs::read(&cert)?,
            key_pem: std::fs::read(&key)?,
            source: cert.display().to_string(),
        });
    }
    let generated = rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into(), "::1".into()])?;
    let cert_pem = generated.cert.pem();
    if let Ok(out) = std::env::var("AWZY_DEV_IDP_CA_OUT") {
        std::fs::write(&out, &cert_pem).with_context(|| format!("writing {out}"))?;
    }
    Ok(TlsFiles {
        cert_pem: cert_pem.into_bytes(),
        key_pem: generated.key_pair.serialize_pem().into_bytes(),
        source: "self-signed".into(),
    })
}

/// Serves `app` over HTTPS on `addr` until the process exits.
pub async fn serve(app: Router, addr: SocketAddr, tls: &TlsFiles) -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let certs = CertificateDer::pem_slice_iter(&tls.cert_pem).collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_slice(&tls.key_pem)?;
    let mut config = rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, cert = %tls.source, "listening (https)");
    loop {
        let (tcp, _) = listener.accept().await?;
        let (acceptor, app) = (acceptor.clone(), app.clone());
        tokio::spawn(async move {
            let Ok(stream) = acceptor.accept(tcp).await else { return };
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), TowerToHyperService::new(app))
                .await;
        });
    }
}

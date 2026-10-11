//! How awzy's services reach Kafka: the brokers, and whether to speak TLS to them (ADR 0091).
//!
//! Plaintext unless `AWZY_KAFKA_TLS` is `true`; the local stack runs plaintext, installs run
//! TLS. Brokers are trusted by the Mozilla roots, which include Amazon's for MSK, and by any PEM
//! certificates in `AWZY_KAFKA_TLS_CA_FILE`, for brokers behind a private CA.

use std::sync::Arc;

use rskafka::client::{Client, ClientBuilder};
use rustls_pki_types::{pem::PemObject, CertificateDer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{name} must be `true` or `false`, not {value:?}")]
    Flag { name: &'static str, value: String },
    #[error("reading the Kafka CA file {path}: {source}")]
    CaFile { path: String, source: std::io::Error },
    #[error("the Kafka CA file {path} {reason}")]
    Ca { path: String, reason: String },
    #[error(transparent)]
    Kafka(#[from] rskafka::client::error::Error),
}

/// A set of brokers and how to connect to them.
#[derive(Clone)]
pub struct Brokers {
    addrs: Vec<String>,
    tls: Option<Arc<rustls::ClientConfig>>,
}

impl std::fmt::Debug for Brokers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Brokers").field("addrs", &self.addrs).field("tls", &self.tls.is_some()).finish()
    }
}

impl Brokers {
    /// Comma-separated `host:port`s, reached in plaintext. For tests and the local stack.
    pub fn plaintext(list: &str) -> Self {
        Self { addrs: split(list), tls: None }
    }

    /// The brokers in the variable `var` (comma-separated `host:port`s), with the TLS settings
    /// above. None when `var` is unset or empty.
    pub fn from_env(var: &str) -> Result<Option<Self>, Error> {
        let list = std::env::var(var).unwrap_or_default();
        if list.trim().is_empty() {
            return Ok(None);
        }
        let tls = match std::env::var("AWZY_KAFKA_TLS").as_deref() {
            Err(_) | Ok("") | Ok("false") => false,
            Ok("true") => true,
            Ok(other) => return Err(Error::Flag { name: "AWZY_KAFKA_TLS", value: other.into() }),
        };
        let ca_file = std::env::var("AWZY_KAFKA_TLS_CA_FILE").ok().filter(|p| !p.is_empty());
        let tls = if tls { Some(tls_config(ca_file.as_deref())?) } else { None };
        Ok(Some(Self { addrs: split(&list), tls }))
    }

    pub fn addrs(&self) -> &[String] {
        &self.addrs
    }

    pub fn is_tls(&self) -> bool {
        self.tls.is_some()
    }

    pub async fn connect(&self) -> Result<Client, Error> {
        let mut builder = ClientBuilder::new(self.addrs.clone());
        if let Some(tls) = &self.tls {
            builder = builder.tls_config(tls.clone());
        }
        Ok(builder.build().await?)
    }
}

fn split(list: &str) -> Vec<String> {
    list.split(',').map(str::trim).filter(|a| !a.is_empty()).map(str::to_string).collect()
}

fn tls_config(ca_file: Option<&str>) -> Result<Arc<rustls::ClientConfig>, Error> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    if let Some(path) = ca_file {
        let ca = |reason: String| Error::Ca { path: path.into(), reason };
        let pem = std::fs::read(path).map_err(|source| Error::CaFile { path: path.into(), source })?;
        let certs = CertificateDer::pem_slice_iter(&pem)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ca(format!("isn't PEM: {e}")))?;
        if certs.is_empty() {
            return Err(ca("has no certificates".into()));
        }
        for cert in certs {
            roots.add(cert).map_err(|e| ca(format!("has a certificate rustls refuses: {e}")))?;
        }
    }
    Ok(Arc::new(rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broker_list_is_split_and_trimmed() {
        let brokers = Brokers::plaintext(" b-1:9094, b-2:9094,,");
        assert_eq!(brokers.addrs(), ["b-1:9094", "b-2:9094"]);
        assert!(!brokers.is_tls());
    }

    #[test]
    fn a_private_ca_is_trusted_alongside_the_public_roots() {
        let ca = rcgen::generate_simple_self_signed(vec!["kafka.internal".into()]).unwrap();
        let path = std::env::temp_dir().join(format!("awzy-kafka-test-ca-{}.pem", std::process::id()));
        std::fs::write(&path, ca.cert.pem()).unwrap();
        assert!(tls_config(Some(path.to_str().unwrap())).is_ok());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_ca_file_without_certificates_is_refused() {
        let path = std::env::temp_dir().join(format!("awzy-kafka-test-empty-{}.pem", std::process::id()));
        std::fs::write(&path, "not a certificate\n").unwrap();
        let err = tls_config(Some(path.to_str().unwrap())).unwrap_err();
        std::fs::remove_file(path).unwrap();
        assert!(matches!(err, Error::Ca { .. }), "{err}");
    }

    #[test]
    fn a_missing_ca_file_is_an_error() {
        assert!(matches!(tls_config(Some("/nonexistent/awzy-ca.pem")), Err(Error::CaFile { .. })));
    }
}

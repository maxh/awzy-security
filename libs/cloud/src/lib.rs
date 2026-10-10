//! The only place awzy knows which cloud it runs on.
//!
//! Services depend on the traits here, never on a cloud SDK. Helm values set `AWZY_CLOUD`
//! and the matching adapters (S3/GCS/Blob, KMS/Cloud KMS/Key Vault, ...) are wired at startup.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudProvider {
    Aws,
    Gcp,
    Azure,
}

#[derive(Debug, thiserror::Error)]
#[error("unknown cloud provider {0:?}; expected aws, gcp, or azure")]
pub struct UnknownCloud(String);

impl FromStr for CloudProvider {
    type Err = UnknownCloud;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "aws" => Ok(Self::Aws),
            "gcp" | "google" => Ok(Self::Gcp),
            "azure" => Ok(Self::Azure),
            _ => Err(UnknownCloud(s.to_string())),
        }
    }
}

/// Envelope encryption and signing through the cloud's key service.
pub trait KeyManager: Send + Sync {
    fn encrypt(&self, plaintext: &[u8]) -> impl std::future::Future<Output = anyhow_like::Result<Vec<u8>>> + Send;
    fn decrypt(&self, ciphertext: &[u8]) -> impl std::future::Future<Output = anyhow_like::Result<Vec<u8>>> + Send;
}

/// Boxed error alias so adapters can return any SDK error without this crate depending on them.
pub mod anyhow_like {
    pub type Error = Box<dyn std::error::Error + Send + Sync>;
    pub type Result<T> = std::result::Result<T, Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_providers() {
        assert_eq!("AWS".parse::<CloudProvider>().unwrap(), CloudProvider::Aws);
        assert_eq!("google".parse::<CloudProvider>().unwrap(), CloudProvider::Gcp);
        assert!("oracle".parse::<CloudProvider>().is_err());
    }
}

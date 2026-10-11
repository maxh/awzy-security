//! Writing a connector's credential to AWS Secrets Manager (ADR 0104). The client portal's role
//! may `PutSecretValue` on the connectors' secrets and nothing else, so this writes and never
//! reads.

use aws_config::{BehaviorVersion, Region};
use aws_sdk_secretsmanager::Client;
use aws_smithy_http_client::tls;
use awzy_cloud::{anyhow_like, SecretWriter};

pub struct SecretsManagerWriter {
    client: Client,
}

impl SecretsManagerWriter {
    /// For secrets in `region`, with credentials from the environment: in an install, the
    /// portal's Pod Identity.
    pub async fn connect(region: &str) -> Self {
        let https = aws_smithy_http_client::Builder::new()
            .tls_provider(tls::Provider::Rustls(tls::rustls_provider::CryptoMode::Ring))
            .build_https();
        let aws = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .http_client(https)
            .load()
            .await;
        Self { client: Client::new(&aws) }
    }
}

/// The region a Secrets Manager ARN names: `arn:aws:secretsmanager:<region>:<account>:secret:<name>`.
pub fn region_of(arn: &str) -> Option<&str> {
    let mut parts = arn.split(':');
    (parts.next() == Some("arn") && parts.next().is_some() && parts.next() == Some("secretsmanager"))
        .then(|| parts.next())
        .flatten()
        .filter(|r| !r.is_empty())
}

impl SecretWriter for SecretsManagerWriter {
    async fn put(&self, secret: &str, value: &str) -> anyhow_like::Result<()> {
        self.client
            .put_secret_value()
            .secret_id(secret)
            .secret_string(value)
            .send()
            .await
            .map_err(|e| -> anyhow_like::Error { Box::new(e.into_service_error()) })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_region_comes_from_the_arn() {
        assert_eq!(
            region_of("arn:aws:secretsmanager:us-east-2:123456789012:secret:awzy/connectors/jira-AbCdEf"),
            Some("us-east-2")
        );
        assert_eq!(region_of("arn:aws:kms:us-east-2:123456789012:key/1"), None, "not a secret");
        assert_eq!(region_of("awzy/connectors/jira"), None);
    }
}

//! Configuring connectors in the portal (ADR 0104). Settings go to awzy-context, which
//! awzy-connectors reads; a credential goes to the cloud's secrets manager, which the portal may
//! write and never read, and which External Secrets syncs to the connectors. Changing either is
//! `configure_connectors` on the enforcement area, which policy can challenge: the base policies
//! ask for a security key.

use async_graphql::{Context, Enum, InputObject, Object, Result, SimpleObject};
use awzy_cloud::SecretWriter;
use awzy_model::ChallengeId;
use awzy_portal::{JsonObject, Qid, UtcTimestamp};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::{
    access::{require, PortalAction, PortalArea},
    directory::context,
    enforcement::action_challenge,
};

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum ConnectorName {
    /// Google Drive's shared drives (ADR 0069).
    Drive,
    /// Jira's issues (ADR 0070).
    Jira,
}

impl ConnectorName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Drive => "drive",
            Self::Jira => "jira",
        }
    }

    /// Where this install keeps its credential, from the chart; unset where it can't keep one.
    fn secret(self) -> Option<String> {
        let var = match self {
            Self::Drive => "AWZY_PORTAL_DRIVE_SECRET",
            Self::Jira => "AWZY_PORTAL_JIRA_SECRET",
        };
        std::env::var(var).ok().filter(|s| !s.is_empty())
    }

    /// Whether `credential` looks like this connector's, so a paste of the wrong thing is caught
    /// here and not at the connector's next round.
    fn check(self, credential: &str) -> Result<(), String> {
        match self {
            Self::Drive => {
                let key: Value = serde_json::from_str(credential)
                    .map_err(|_| "Drive's credential is the service account's JSON key".to_string())?;
                if key["type"] != "service_account"
                    || !key["private_key"].is_string()
                    || !key["client_email"].is_string()
                {
                    return Err("that isn't a Google service account key".into());
                }
                Ok(())
            }
            Self::Jira if credential.trim().is_empty() || credential.trim().contains(char::is_whitespace) => {
                Err("Jira's credential is the account's API token".into())
            }
            Self::Jira => Ok(()),
        }
    }
}

/// A connector as configured in the portal.
#[derive(SimpleObject, Debug, Clone, PartialEq)]
pub struct ConnectorConfig {
    pub name: ConnectorName,
    pub enabled: bool,
    /// Drive: `subject`, `admin_subject`, `domain`. Jira: `url`, `user`, `projects`, `watchers`.
    /// Either: `interval_secs`.
    pub settings: JsonObject,
    /// When its credential was last set here; null if never. The credential itself is never
    /// shown.
    pub credential_set_at: Option<UtcTimestamp>,
    pub updated_by: String,
    pub updated_at: UtcTimestamp,
}

fn time(v: &Value) -> Option<UtcTimestamp> {
    v.as_str().and_then(|t| t.parse::<DateTime<Utc>>().ok()).map(UtcTimestamp)
}

fn config(c: &Value) -> Option<ConnectorConfig> {
    let name = match c["name"].as_str()? {
        "drive" => ConnectorName::Drive,
        "jira" => ConnectorName::Jira,
        _ => return None,
    };
    Some(ConnectorConfig {
        name,
        enabled: c["enabled"].as_bool().unwrap_or(false),
        settings: JsonObject::from_json(c["settings"].clone()).unwrap_or_default(),
        credential_set_at: time(&c["credential_set_at"]),
        updated_by: c["updated_by"].as_str().unwrap_or_default().to_string(),
        updated_at: time(&c["updated_at"]).unwrap_or(UtcTimestamp(Utc::now())),
    })
}

#[derive(InputObject)]
pub struct ConfigureConnectorInput {
    pub name: ConnectorName,
    #[graphql(default = true)]
    pub enabled: bool,
    pub settings: JsonObject,
    /// A new credential: Drive's service account key (its JSON), or Jira's API token. Omit it to
    /// keep the one already set.
    pub credential: Option<String>,
}

#[derive(SimpleObject)]
pub struct ConfigureConnectorPayload {
    /// The connector as saved; null when a challenge must be passed first.
    pub connector: Option<ConnectorConfig>,
    /// The challenge to pass, then save again.
    pub challenge_qid: Option<Qid<ChallengeId>>,
}

#[derive(Default)]
pub struct ConnectorConfigQuery;

#[Object]
impl ConnectorConfigQuery {
    /// The connectors configured in the portal. For those who may view the enforcement area.
    async fn connector_configs(&self, ctx: &Context<'_>) -> Result<Vec<ConnectorConfig>> {
        require(ctx, PortalAction::View, PortalArea::Enforcement).await?;
        let found: Value = context(ctx)?.get("/v1/connector-settings").await?;
        Ok(found["connectors"].as_array().into_iter().flatten().filter_map(config).collect())
    }

    /// Whether this install can keep connector credentials (its cloud's secrets manager is set
    /// up), so the portal can say why it can't.
    async fn connector_credentials_supported(&self, ctx: &Context<'_>) -> Result<bool> {
        require(ctx, PortalAction::View, PortalArea::Enforcement).await?;
        Ok(ConnectorName::Drive.secret().is_some() && ConnectorName::Jira.secret().is_some())
    }
}

#[derive(Default)]
pub struct ConnectorConfigMutation;

/// Saves `name`'s settings in awzy-context, which checks them.
async fn save(
    ctx: &Context<'_>,
    name: ConnectorName,
    enabled: bool,
    settings: &JsonObject,
    credential_set: bool,
    viewer: &crate::access::Viewer,
) -> Result<ConnectorConfig> {
    let by = viewer.actor().label();
    let body = json!({
        "enabled": enabled,
        "settings": Value::Object(settings.0.clone()),
        "credential_set": credential_set,
        "updated_by": by,
    });
    let path = format!("/v1/connector-settings/{}", name.as_str());
    let (status, res) = context(ctx)?.call("PUT", &path, Some(&viewer.actor().to_string()), Some(body)).await?;
    match status {
        200 => {
            let saved: Value = serde_json::from_slice(&res)?;
            Ok(config(&saved["connector"]).ok_or("awzy-context returned no connector")?)
        }
        400 => Err(String::from_utf8_lossy(&res).into_owned().into()),
        _ => Err(format!("awzy-context answered {status}").into()),
    }
}

#[Object]
impl ConnectorConfigMutation {
    /// Configures a connector, and sets its credential if one is given. awzy-connectors picks the
    /// change up within a minute or so. For those who may administer the enforcement area; policy
    /// can ask for a security key first, as the base policies do: then nothing is saved, and the
    /// payload names the challenge to pass before saving again.
    async fn configure_connector(
        &self,
        ctx: &Context<'_>,
        input: ConfigureConnectorInput,
    ) -> Result<ConfigureConnectorPayload> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Enforcement).await?;
        if let Some(challenge_qid) =
            action_challenge(ctx, &viewer, "configure_connectors", "configuring a connector").await?
        {
            return Ok(ConfigureConnectorPayload { connector: None, challenge_qid: Some(challenge_qid) });
        }
        let name = input.name;
        let credential = input.credential.as_deref().map(str::trim).filter(|c| !c.is_empty());
        if let Some(c) = credential {
            name.check(c)?;
        }
        // The settings first, so ones awzy-context refuses don't leave a new credential behind.
        let mut saved = save(ctx, name, input.enabled, &input.settings, false, &viewer).await?;
        if let Some(c) = credential {
            let secret = name.secret().ok_or("this install can't keep connector credentials")?;
            let region =
                awzy_secrets_aws::region_of(&secret).ok_or("the connector's secret isn't in AWS Secrets Manager")?;
            awzy_secrets_aws::SecretsManagerWriter::connect(region)
                .await
                .put(&secret, c)
                .await
                .map_err(|e| format!("the settings were saved, but not the credential: {e}"))?;
            saved = save(ctx, name, input.enabled, &input.settings, true, &viewer).await?;
        }
        tracing::warn!(connector = name.as_str(), enabled = input.enabled, credential = credential.is_some(), by = %viewer.actor().label(), "connector configured");
        Ok(ConfigureConnectorPayload { connector: Some(saved), challenge_qid: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_has_to_look_like_the_connectors() {
        let key = r#"{"type":"service_account","client_email":"sync@p.iam.gserviceaccount.com","private_key":"-----BEGIN PRIVATE KEY-----"}"#;
        assert!(ConnectorName::Drive.check(key).is_ok());
        assert!(ConnectorName::Drive.check("ATATT3x-token").is_err(), "a Jira token isn't a Drive key");
        assert!(ConnectorName::Drive.check(r#"{"type":"authorized_user"}"#).is_err());
        assert!(ConnectorName::Jira.check("ATATT3xFfGF0").is_ok());
        assert!(ConnectorName::Jira.check("two words").is_err());
    }

    #[test]
    fn a_saved_connector_reads_back_from_awzy_context() {
        let c = config(&json!({
            "name": "jira", "enabled": true, "settings": { "url": "https://acme.atlassian.net", "user": "bot" },
            "credential_set_at": "2026-10-11T12:00:00Z", "updated_by": "Human::alice", "updated_at": "2026-10-11T12:00:00Z",
        }))
        .unwrap();
        assert_eq!((c.name, c.enabled, c.settings.0["user"].as_str()), (ConnectorName::Jira, true, Some("bot")));
        assert!(c.credential_set_at.is_some());
        assert!(config(&json!({ "name": "slack" })).is_none());
    }
}

//! Configuring connectors in the portal (ADR 0104). Settings go to awzy-context, which
//! awzy-connectors reads; a credential goes to the cloud's secrets manager, which the portal may
//! write and never read, and which External Secrets syncs to the connectors. Changing either is
//! `configure_connectors` on the enforcement area, which policy can challenge: the base policies
//! ask for a security key.

use async_graphql::{Context, Enum, InputObject, Object, Result, SimpleObject};
use awzy_cloud::SecretWriter;
use awzy_context_api::{
    model::{ConnectorSettings, PutConnectorSettings},
    ContextError,
};
use awzy_model::ChallengeId;
use awzy_portal::{ErrorCode, JsonObject, Qid, UtcTimestamp};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    access::{require, PortalAction, PortalArea},
    directory::context_api,
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
    fn check(self, credential: &str) -> Result<(), async_graphql::Error> {
        /// The fields of a Google service account key the Drive connector signs in with.
        #[derive(Deserialize)]
        struct ServiceAccountKey {
            r#type: String,
            #[expect(dead_code, reason = "only checked to be there")]
            private_key: String,
            #[expect(dead_code, reason = "only checked to be there")]
            client_email: String,
        }
        let invalid = |m: &str| Err(ErrorCode::InvalidInput.error(m));
        match self {
            Self::Drive => {
                let Ok(key) = serde_json::from_str::<Value>(credential) else {
                    return invalid("Drive's credential is the service account's JSON key");
                };
                match serde_json::from_value::<ServiceAccountKey>(key) {
                    Ok(k) if k.r#type == "service_account" => Ok(()),
                    _ => invalid("that isn't a Google service account key"),
                }
            }
            Self::Jira if credential.trim().is_empty() || credential.trim().contains(char::is_whitespace) => {
                invalid("Jira's credential is the account's API token")
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

/// A connector's settings as awzy-context keeps them, for the portal; None for a connector this
/// portal doesn't know.
fn config(c: ConnectorSettings) -> Option<ConnectorConfig> {
    let name = match c.name.as_str() {
        "drive" => ConnectorName::Drive,
        "jira" => ConnectorName::Jira,
        _ => return None,
    };
    Some(ConnectorConfig {
        name,
        enabled: c.enabled,
        settings: JsonObject::from_json(c.settings).unwrap_or_default(),
        credential_set_at: c.credential_set_at.map(UtcTimestamp),
        updated_by: c.updated_by,
        updated_at: UtcTimestamp(c.updated_at),
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
        Ok(context_api(ctx)?.connector_settings().await?.into_iter().filter_map(config).collect())
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
    let put = PutConnectorSettings {
        enabled,
        settings: Value::Object(settings.0.clone()),
        credential_set,
        updated_by: viewer.actor().label(),
    };
    let saved = context_api(ctx)?
        .put_connector_settings(name.as_str(), &put, &viewer.actor().to_string())
        .await
        .map_err(|e| match e {
            // awzy-context's reason, as it gave it: which setting is wrong.
            ContextError::Invalid(why) => async_graphql::Error::new(why),
            e => e.into(),
        })?;
    Ok(config(saved).ok_or("awzy-context returned a connector this portal doesn't know")?)
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
        let at = "2026-10-11T12:00:00Z".parse().unwrap();
        let saved = |name: &str| ConnectorSettings {
            name: name.into(),
            enabled: true,
            settings: serde_json::json!({ "url": "https://acme.atlassian.net", "user": "bot" }),
            credential_set_at: Some(at),
            updated_by: "alice".into(),
            updated_at: at,
        };
        let c = config(saved("jira")).unwrap();
        assert_eq!((c.name, c.enabled, c.settings.0["user"].as_str()), (ConnectorName::Jira, true, Some("bot")));
        assert!(c.credential_set_at.is_some());
        assert!(config(saved("slack")).is_none());
    }
}

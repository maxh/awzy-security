//! Each connector's settings, set in the client portal and read by awzy-connectors (ADR 0104).
//! `PUT /v1/connector-settings/{name}` from the portal; `GET /v1/connector-settings` and
//! `/v1/connector-settings/{name}` for both. Credentials live in the cloud's secrets manager;
//! this only records when the portal last wrote one.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;

use crate::store::ContextStore;

/// The connectors awzy has.
pub const CONNECTORS: &[&str] = &["drive", "jira"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConnectorSettings {
    pub name: String,
    pub enabled: bool,
    pub settings: Value,
    pub credential_set_at: Option<DateTime<Utc>>,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct PutSettings {
    pub enabled: bool,
    pub settings: Value,
    /// The portal wrote a new credential with this change.
    #[serde(default)]
    pub credential_set: bool,
    pub updated_by: String,
}

fn text<'a>(s: &'a Value, key: &str) -> Option<&'a str> {
    s[key].as_str().map(str::trim).filter(|v| !v.is_empty())
}

fn interval(s: &Value) -> Result<(), String> {
    match s.get("interval_secs") {
        None | Some(Value::Null) => Ok(()),
        Some(v) => match v.as_u64() {
            Some(n) if (60..=86_400).contains(&n) => Ok(()),
            _ => Err("interval_secs is 60 to 86400".into()),
        },
    }
}

/// What `name`'s settings must hold, so a connector never starts on settings it can't use.
pub fn validate(name: &str, s: &Value) -> Result<(), String> {
    if !s.is_object() {
        return Err("settings is an object".into());
    }
    interval(s)?;
    match name {
        "drive" => {
            let subject = text(s, "subject").ok_or("subject is the user the connector acts as")?;
            if text(s, "domain").is_none() && !subject.contains('@') {
                return Err("domain is needed when subject isn't an email address".into());
            }
            Ok(())
        }
        "jira" => {
            let url = text(s, "url").ok_or("url is the Jira site, https://<site>.atlassian.net")?;
            if !url.starts_with("https://") {
                return Err("url starts with https://".into());
            }
            text(s, "user").ok_or("user is the account the connector signs in as")?;
            if let Some(p) = s.get("projects").filter(|p| !p.is_null()) {
                if !p.as_array().is_some_and(|a| a.iter().all(Value::is_string)) {
                    return Err("projects is a list of project keys".into());
                }
            }
            if s.get("watchers").is_some_and(|w| !w.is_null() && !w.is_boolean()) {
                return Err("watchers is true or false".into());
            }
            Ok(())
        }
        other => Err(format!("{other:?} isn't a connector")),
    }
}

const COLUMNS: &str = "name, enabled, settings, credential_set_at, updated_by, updated_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<ConnectorSettings, sqlx::Error> {
    Ok(ConnectorSettings {
        name: row.try_get("name")?,
        enabled: row.try_get("enabled")?,
        settings: row.try_get::<sqlx::types::Json<Value>, _>("settings")?.0,
        credential_set_at: row.try_get("credential_set_at")?,
        updated_by: row.try_get("updated_by")?,
        updated_at: row.try_get("updated_at")?,
    })
}

impl ContextStore {
    pub async fn connector_settings(&self) -> Result<Vec<ConnectorSettings>, sqlx::Error> {
        let rows = sqlx::query(&format!("SELECT {COLUMNS} FROM connector_settings ORDER BY name"))
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(from_row).collect()
    }

    /// Replaces `name`'s settings, which the caller has validated.
    pub async fn put_connector_settings(
        &self,
        name: &str,
        put: &PutSettings,
    ) -> Result<ConnectorSettings, sqlx::Error> {
        let row = sqlx::query(&format!(
            "INSERT INTO connector_settings (name, enabled, settings, credential_set_at, updated_by)
             VALUES ($1, $2, $3, CASE WHEN $4 THEN now() END, $5)
             ON CONFLICT (name) DO UPDATE SET enabled = $2, settings = $3, updated_by = $5, updated_at = now(),
                 credential_set_at = CASE WHEN $4 THEN now() ELSE connector_settings.credential_set_at END
             RETURNING {COLUMNS}"
        ))
        .bind(name)
        .bind(put.enabled)
        .bind(sqlx::types::Json(&put.settings))
        .bind(put.credential_set)
        .bind(&put.updated_by)
        .fetch_one(self.pool())
        .await?;
        from_row(&row)
    }
}

fn storage_error(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "connector settings storage error");
    (StatusCode::SERVICE_UNAVAILABLE, "storage error").into_response()
}

async fn list(State(store): State<ContextStore>) -> Response {
    match store.connector_settings().await {
        Ok(all) => Json(json!({ "connectors": all })).into_response(),
        Err(e) => storage_error(e),
    }
}

async fn get_one(State(store): State<ContextStore>, Path(name): Path<String>) -> Response {
    match store.connector_settings().await {
        Ok(all) => match all.into_iter().find(|c| c.name == name) {
            Some(c) => Json(json!({ "connector": c })).into_response(),
            None => (StatusCode::NOT_FOUND, format!("{name} isn't configured")).into_response(),
        },
        Err(e) => storage_error(e),
    }
}

async fn put_one(
    State(store): State<ContextStore>,
    Path(name): Path<String>,
    Json(put): Json<PutSettings>,
) -> Response {
    if !CONNECTORS.contains(&name.as_str()) {
        return (StatusCode::NOT_FOUND, format!("{name:?} isn't a connector")).into_response();
    }
    if let Err(e) = validate(&name, &put.settings) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    if put.updated_by.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "updated_by is required").into_response();
    }
    match store.put_connector_settings(&name, &put).await {
        Ok(c) => Json(json!({ "connector": c })).into_response(),
        Err(e) => storage_error(e),
    }
}

/// For awzy-connectors and the client portal.
pub fn read_router(store: ContextStore) -> Router {
    Router::new()
        .route("/v1/connector-settings", get(list))
        .route("/v1/connector-settings/{name}", get(get_one))
        .with_state(store)
}

/// For the client portal, which authorizes the change.
pub fn write_router(store: ContextStore) -> Router {
    Router::new().route("/v1/connector-settings/{name}", axum::routing::put(put_one)).with_state(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_a_connector_cant_use_are_refused() {
        assert!(validate("drive", &json!({ "subject": "sync@example.com" })).is_ok());
        assert!(validate("drive", &json!({ "subject": "sync" })).is_err(), "no domain to tell");
        assert!(validate("drive", &json!({ "subject": "sync", "domain": "example.com" })).is_ok());
        assert!(validate("jira", &json!({ "url": "https://acme.atlassian.net", "user": "bot@acme.com" })).is_ok());
        assert!(validate("jira", &json!({ "url": "http://acme.atlassian.net", "user": "bot" })).is_err());
        assert!(validate("jira", &json!({ "url": "https://a.atlassian.net", "user": "b", "projects": "OPS" })).is_err());
        assert!(
            validate("jira", &json!({ "url": "https://a.atlassian.net", "user": "b", "interval_secs": 5 })).is_err()
        );
        assert!(validate("slack", &json!({})).is_err());
    }

    #[tokio::test]
    async fn settings_replace_and_remember_when_the_credential_was_set() {
        let Some(store) = crate::store::tests::test_store().await else { return };
        let put = |credential_set, user: &str| PutSettings {
            enabled: true,
            settings: json!({ "url": "https://acme.atlassian.net", "user": user }),
            credential_set,
            updated_by: "alice".into(),
        };
        let first = store.put_connector_settings("jira", &put(true, "bot")).await.unwrap();
        assert!(first.credential_set_at.is_some());
        let second = store.put_connector_settings("jira", &put(false, "bot2")).await.unwrap();
        assert_eq!(second.settings["user"], "bot2");
        assert_eq!(second.credential_set_at, first.credential_set_at, "kept when the credential didn't change");
        assert_eq!(store.connector_settings().await.unwrap(), vec![second]);
    }
}

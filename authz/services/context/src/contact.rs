//! How to reach a person (#65): the email their IdP provisioned over SCIM, so awzy-challenge can
//! message people whose `userName` isn't an email address.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::store::ContextStore;

/// One of a SCIM user's `emails`.
#[derive(Deserialize)]
struct Email {
    value: String,
    #[serde(default)]
    primary: bool,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize, Default)]
struct Emails {
    #[serde(default)]
    emails: Vec<Email>,
}

/// The address to message a person at: their primary SCIM email, else their work one, else
/// the first; else their `userName` when it's an address itself.
pub fn email_of(user_name: &str, resource: &Value) -> Option<String> {
    let emails = Emails::deserialize(resource).unwrap_or_default().emails;
    let usable = |e: &&Email| e.value.contains('@');
    emails
        .iter()
        .filter(usable)
        .find(|e| e.primary)
        .or_else(|| emails.iter().filter(usable).find(|e| e.kind.as_deref() == Some("work")))
        .or_else(|| emails.iter().find(usable))
        .map(|e| e.value.trim().to_string())
        .or_else(|| user_name.contains('@').then(|| user_name.to_string()))
}

impl ContextStore {
    /// An active person's SCIM resource, by `userName` (ignoring case).
    pub async fn human_resource(&self, user_name: &str) -> Result<Option<(String, Value)>, sqlx::Error> {
        let row: Option<(String, sqlx::types::Json<Value>)> =
            sqlx::query_as("SELECT user_name, resource FROM humans WHERE lower(user_name) = lower($1) AND active")
                .bind(user_name)
                .fetch_optional(self.pool())
                .await?;
        Ok(row.map(|(u, r)| (u, r.0)))
    }
}

/// `GET /v1/humans/{user_name}/contact`: `{ user_name, email }`, email null when there's none;
/// 404 for someone the IdP hasn't provisioned, or deactivated.
async fn contact(State(store): State<ContextStore>, Path(user_name): Path<String>) -> Response {
    match store.human_resource(&user_name).await {
        Ok(Some((user_name, resource))) => {
            let email = email_of(&user_name, &resource);
            Json(awzy_context_api::model::HumanContact { user_name, email }).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "awzy-context: reading a person's contact failed");
            (StatusCode::SERVICE_UNAVAILABLE, "storage error").into_response()
        }
    }
}

pub fn router(store: ContextStore) -> Router {
    Router::new().route("/v1/humans/{user_name}/contact", get(contact)).with_state(store)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_primary_email_wins_then_work_then_any_then_the_user_name() {
        let emails = |e: Value| json!({ "userName": "nora", "emails": e });
        assert_eq!(
            email_of(
                "nora",
                &emails(json!([{ "value": "n@home.example" }, { "value": "nora@larkspur.example", "primary": true }]))
            ),
            Some("nora@larkspur.example".into())
        );
        assert_eq!(
            email_of(
                "nora",
                &emails(
                    json!([{ "value": "n@home.example", "type": "home" }, { "value": "nora@larkspur.example", "type": "work" }])
                )
            ),
            Some("nora@larkspur.example".into())
        );
        assert_eq!(email_of("nora", &emails(json!([{ "value": "n@home.example" }]))), Some("n@home.example".into()));
        assert_eq!(email_of("nora@larkspur.example", &json!({})), Some("nora@larkspur.example".into()));
        assert_eq!(email_of("nora", &json!({ "emails": "not a list" })), None, "nothing to message");
    }
}

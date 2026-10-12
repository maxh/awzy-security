//! Apps an admin trusts to sign tokens for their own agents (ADR 0110). `PUT` and `DELETE
//! /v1/token-issuers/{name}` from the client portal, which authorizes the change; the list goes to
//! the PDPs with the principals snapshot, and from them to every enforcement point.

use awzy_model::TrustedIssuer;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;

use crate::{
    store::{bump, ContextStore},
    Invalid,
};

/// A trusted issuer as the portal lists it: who registered it and when.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IssuerRecord {
    #[serde(flatten)]
    pub issuer: TrustedIssuer,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct PutIssuer {
    pub issuer: String,
    pub jwks_url: String,
    pub audience: String,
}

const COLUMNS: &str = "name, issuer, jwks_url, audience, created_by, created_at, updated_by, updated_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<IssuerRecord, sqlx::Error> {
    Ok(IssuerRecord {
        issuer: TrustedIssuer {
            name: row.try_get("name")?,
            issuer: row.try_get("issuer")?,
            jwks_url: row.try_get("jwks_url")?,
            audience: row.try_get("audience")?,
        },
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_by: row.try_get("updated_by")?,
        updated_at: row.try_get("updated_at")?,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum IssuerError {
    #[error(transparent)]
    Invalid(#[from] Invalid),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl ContextStore {
    pub async fn token_issuers(&self) -> Result<Vec<IssuerRecord>, sqlx::Error> {
        let rows =
            sqlx::query(&format!("SELECT {COLUMNS} FROM token_issuers ORDER BY name")).fetch_all(self.pool()).await?;
        rows.iter().map(from_row).collect()
    }

    /// Registers an app as an issuer, or changes one. Its `iss` can't be another app's.
    pub async fn put_token_issuer(&self, issuer: &TrustedIssuer, by: &str) -> Result<IssuerRecord, IssuerError> {
        issuer.validate().map_err(|e| Invalid(e.to_string()))?;
        let mut tx = self.pool().begin().await?;
        let taken: Option<String> = sqlx::query_scalar("SELECT name FROM token_issuers WHERE issuer = $1 AND name <> $2")
            .bind(&issuer.issuer)
            .bind(&issuer.name)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(other) = taken {
            return Err(Invalid(format!("{other} already signs tokens as {:?}", issuer.issuer)).into());
        }
        let row = sqlx::query(&format!(
            "INSERT INTO token_issuers (name, issuer, jwks_url, audience, created_by, updated_by) VALUES ($1, $2, $3, $4, $5, $5)
             ON CONFLICT (name) DO UPDATE SET issuer = $2, jwks_url = $3, audience = $4, updated_by = $5, updated_at = now()
             RETURNING {COLUMNS}"
        ))
        .bind(&issuer.name)
        .bind(&issuer.issuer)
        .bind(&issuer.jwks_url)
        .bind(&issuer.audience)
        .bind(by)
        .fetch_one(&mut *tx)
        .await?;
        bump(&mut tx).await?;
        tx.commit().await?;
        Ok(from_row(&row)?)
    }

    /// Stops trusting an app's tokens. Its agents stay registered until they're removed.
    pub async fn delete_token_issuer(&self, name: &str) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool().begin().await?;
        let deleted =
            sqlx::query("DELETE FROM token_issuers WHERE name = $1").bind(name).execute(&mut *tx).await?.rows_affected()
                > 0;
        if deleted {
            bump(&mut tx).await?;
        }
        tx.commit().await?;
        Ok(deleted)
    }
}

fn storage_error(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "token issuer storage error");
    (StatusCode::SERVICE_UNAVAILABLE, "storage error").into_response()
}

async fn list(State(store): State<ContextStore>) -> Response {
    match store.token_issuers().await {
        Ok(all) => Json(json!({ "token_issuers": all })).into_response(),
        Err(e) => storage_error(e),
    }
}

async fn put_one(
    State(store): State<ContextStore>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(put): Json<PutIssuer>,
) -> Response {
    let by = crate::actor(&headers);
    let issuer = TrustedIssuer { name, issuer: put.issuer, jwks_url: put.jwks_url, audience: put.audience };
    match store.put_token_issuer(&issuer, &by).await {
        Ok(record) => {
            tracing::info!(token_issuer = %record.issuer.name, iss = %record.issuer.issuer, %by, "token issuer registered");
            Json(json!({ "token_issuer": record })).into_response()
        }
        Err(IssuerError::Invalid(e)) => e.into_response(),
        Err(IssuerError::Db(e)) => storage_error(e),
    }
}

async fn delete_one(State(store): State<ContextStore>, Path(name): Path<String>, headers: HeaderMap) -> Response {
    let by = crate::actor(&headers);
    match store.delete_token_issuer(&name).await {
        Ok(true) => {
            tracing::info!(token_issuer = %name, %by, "token issuer removed");
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (StatusCode::NOT_FOUND, "no such token issuer").into_response(),
        Err(e) => storage_error(e),
    }
}

/// For the client portal, which authorizes changes.
pub fn router(store: ContextStore) -> Router {
    Router::new()
        .route("/v1/token-issuers", get(list))
        .route("/v1/token-issuers/{name}", axum::routing::put(put_one).delete(delete_one))
        .with_state(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, iss: &str) -> TrustedIssuer {
        TrustedIssuer {
            name: name.into(),
            issuer: iss.into(),
            jwks_url: format!("{iss}/jwks"),
            audience: "awzy".into(),
        }
    }

    #[tokio::test]
    async fn an_issuer_is_in_the_snapshot_and_no_two_apps_share_an_iss() {
        let Some(store) = crate::store::tests::test_store().await else { return };
        let before = store.version().await.unwrap();
        let crm = store.put_token_issuer(&app("acme-crm", "https://crm.acme.example"), "Human::alice").await.unwrap();
        assert_eq!(crm.created_by, "Human::alice");
        assert!(store.version().await.unwrap() > before, "PDPs fetch the list again");
        let err = store.put_token_issuer(&app("other", "https://crm.acme.example"), "Human::alice").await.unwrap_err();
        assert!(matches!(err, IssuerError::Invalid(_)), "{err}");
        assert_eq!(store.token_issuers().await.unwrap(), vec![crm]);
        assert!(store.delete_token_issuer("acme-crm").await.unwrap());
        assert!(store.token_issuers().await.unwrap().is_empty());
    }
}

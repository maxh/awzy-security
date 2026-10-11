//! Risk scores from awzy-reasoner (ADR 0056): one per person or service account, 0 to 100, with
//! the reasons. Written in batches at `POST /v1/risk`; the PDPs get them with the principals
//! snapshot, as `risk`, keyed `Type::id`, and each check sees its own principal's score as
//! `context.risk_score`.

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use awzy_model::{entity_type, EntityRef};

use crate::store::ContextStore;

/// The most scores one request may carry.
pub const MAX_BATCH: usize = 10_000;

#[derive(Debug, Clone, Deserialize)]
pub struct RiskInput {
    pub principal: EntityRef,
    pub score: i32,
    #[serde(default)]
    pub reasons: Vec<String>,
}

impl RiskInput {
    fn validate(&self) -> Result<(), String> {
        if ![entity_type::HUMAN, entity_type::SERVICE_ACCOUNT].contains(&self.principal.kind.as_str()) {
            return Err(format!("{}: only people and service accounts have risk scores", self.principal.kind));
        }
        if self.principal.id.trim().is_empty() || self.principal.id.len() > 256 {
            return Err("principal.id is 1 to 256 characters".into());
        }
        if !(0..=100).contains(&self.score) {
            return Err(format!("{}: score is 0 to 100, not {}", self.principal.id, self.score));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct RiskBatch {
    scores: Vec<RiskInput>,
}

impl ContextStore {
    /// Replaces these principals' scores and moves the snapshot version, so the PDPs reload.
    pub async fn put_risk(&self, scores: &[RiskInput]) -> Result<usize, sqlx::Error> {
        let mut tx = self.pool().begin().await?;
        for r in scores {
            sqlx::query(
                "INSERT INTO risk_scores (principal_type, principal_id, score, reasons, computed_at)
                 VALUES ($1, $2, $3, $4, now())
                 ON CONFLICT (principal_type, principal_id)
                 DO UPDATE SET score = $3, reasons = $4, computed_at = now()",
            )
            .bind(&r.principal.kind)
            .bind(&r.principal.id)
            .bind(r.score)
            .bind(sqlx::types::Json(&r.reasons))
            .execute(&mut *tx)
            .await?;
        }
        crate::store::bump(&mut tx).await?;
        tx.commit().await?;
        Ok(scores.len())
    }

    /// Every score, keyed `Type::id`, as the snapshot carries them.
    pub async fn risk_scores(&self) -> Result<serde_json::Map<String, Value>, sqlx::Error> {
        let rows: Vec<(String, String, i32)> =
            sqlx::query_as("SELECT principal_type, principal_id, score FROM risk_scores")
                .fetch_all(self.pool())
                .await?;
        Ok(rows.into_iter().map(|(t, id, score)| (format!("{t}::{id}"), json!(score))).collect())
    }
}

async fn put_risk(State(store): State<ContextStore>, Json(batch): Json<RiskBatch>) -> impl IntoResponse {
    if batch.scores.len() > MAX_BATCH {
        return (StatusCode::BAD_REQUEST, format!("at most {MAX_BATCH} scores per request")).into_response();
    }
    if let Some(e) = batch.scores.iter().find_map(|r| r.validate().err()) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    match store.put_risk(&batch.scores).await {
        Ok(n) => Json(json!({ "written": n })).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "writing risk scores failed");
            (StatusCode::SERVICE_UNAVAILABLE, "storage error").into_response()
        }
    }
}

/// `POST /v1/risk`: the reasoner's scores.
pub fn router(store: ContextStore) -> Router {
    Router::new().route("/v1/risk", post(put_risk)).with_state(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::test_store;

    fn score(kind: &str, id: &str, score: i32) -> RiskInput {
        RiskInput { principal: EntityRef::new(kind, id), score, reasons: vec!["test".into()] }
    }

    #[test]
    fn scores_are_for_principals_and_between_0_and_100() {
        assert!(score("Human", "nora", 63).validate().is_ok());
        assert!(score("Resource", "doc", 10).validate().is_err());
        assert!(score("Human", "nora", 101).validate().is_err());
        assert!(score("ServiceAccount", "", 5).validate().is_err());
    }

    #[tokio::test]
    async fn scores_replace_and_move_the_snapshot_version() {
        let Some(store) = test_store().await else { return };
        let before = store.version().await.unwrap();
        store.put_risk(&[score("Human", "nora", 40), score("ServiceAccount", "sales-genie", 80)]).await.unwrap();
        store.put_risk(&[score("Human", "nora", 12)]).await.unwrap();
        let scores = store.risk_scores().await.unwrap();
        assert_eq!(scores["Human::nora"], 12, "replaced");
        assert_eq!(scores["ServiceAccount::sales-genie"], 80);
        assert!(store.version().await.unwrap() > before, "PDPs reload");
    }
}

//! Investigations from awzy-reasoner (ADR 0071): a model's checked finding about one principal
//! (ADR 0067). The reasoner writes each one at `POST /v1/investigations`; it and, later, the
//! client portal read them back, and the reasoner counts their tokens against the monthly budget.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, Row};

use awzy_model::{entity_type, EntityRef, InvestigationId};

use crate::store::ContextStore;

const VERDICTS: &[&str] = &["malicious", "suspicious", "benign", "inconclusive"];
const CONTAINMENTS: &[&str] = &["none", "propose", "recommend"];
const STATUSES: &[&str] = &["needs_review", "done"];
/// The most one list returns.
const MAX_LIST: i64 = 500;

/// One investigation as the reasoner writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationInput {
    pub subject: EntityRef,
    pub trigger: String,
    pub floor: i32,
    pub status: String,
    pub verdict: String,
    pub risk: i32,
    pub summary: String,
    #[serde(default)]
    pub evidence: Value,
    pub containment: String,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub finding: Option<Value>,
    pub model: String,
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    pub started_at: DateTime<Utc>,
}

impl InvestigationInput {
    fn validate(&self) -> Result<(), String> {
        if ![entity_type::HUMAN, entity_type::SERVICE_ACCOUNT].contains(&self.subject.kind.as_str()) {
            return Err(format!("{}: only people and service accounts are investigated", self.subject.kind));
        }
        if self.subject.id.trim().is_empty() || self.subject.id.len() > 256 {
            return Err("subject.id is 1 to 256 characters".into());
        }
        if !(0..=100).contains(&self.floor) || !(0..=100).contains(&self.risk) {
            return Err("floor and risk are 0 to 100".into());
        }
        let one_of = |field: &str, value: &str, allowed: &[&str]| {
            if allowed.contains(&value) {
                Ok(())
            } else {
                Err(format!("{field} is one of {}, not {value:?}", allowed.join(", ")))
            }
        };
        one_of("status", &self.status, STATUSES)?;
        one_of("verdict", &self.verdict, VERDICTS)?;
        one_of("containment", &self.containment, CONTAINMENTS)?;
        if self.input_tokens < 0 || self.output_tokens < 0 {
            return Err("token counts can't be negative".into());
        }
        if !self.evidence.is_array() && !self.evidence.is_null() {
            return Err("evidence is a list".into());
        }
        Ok(())
    }
}

/// One stored investigation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Investigation {
    pub qid: String,
    #[serde(flatten)]
    pub input: InvestigationInput,
    pub finished_at: DateTime<Utc>,
}

/// Tokens spent on investigations since a time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Usage {
    pub investigations: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// What to list.
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub subject_type: Option<String>,
    pub subject_id: Option<String>,
    /// Only those finished after this.
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

fn from_row(row: &PgRow) -> Result<Investigation, sqlx::Error> {
    let notes: sqlx::types::Json<Vec<String>> = row.try_get("notes")?;
    let finding: Option<sqlx::types::Json<Value>> = row.try_get("finding")?;
    let evidence: sqlx::types::Json<Value> = row.try_get("evidence")?;
    Ok(Investigation {
        qid: InvestigationId::from_uuid(row.try_get("id")?).to_string(),
        input: InvestigationInput {
            subject: EntityRef::new(row.try_get::<String, _>("subject_type")?, row.try_get::<String, _>("subject_id")?),
            trigger: row.try_get("trigger")?,
            floor: row.try_get("floor")?,
            status: row.try_get("status")?,
            verdict: row.try_get("verdict")?,
            risk: row.try_get("risk")?,
            summary: row.try_get("summary")?,
            evidence: evidence.0,
            containment: row.try_get("containment")?,
            notes: notes.0,
            finding: finding.map(|f| f.0),
            model: row.try_get("model")?,
            input_tokens: row.try_get("input_tokens")?,
            output_tokens: row.try_get("output_tokens")?,
            started_at: row.try_get("started_at")?,
        },
        finished_at: row.try_get("finished_at")?,
    })
}

impl ContextStore {
    pub async fn put_investigation(&self, i: &InvestigationInput) -> Result<Investigation, sqlx::Error> {
        let row = sqlx::query(
            "INSERT INTO investigations (id, subject_type, subject_id, trigger, floor, status, verdict, risk, summary,
                 evidence, containment, notes, finding, model, input_tokens, output_tokens, started_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
             RETURNING *",
        )
        .bind(InvestigationId::new().uuid())
        .bind(&i.subject.kind)
        .bind(&i.subject.id)
        .bind(&i.trigger)
        .bind(i.floor)
        .bind(&i.status)
        .bind(&i.verdict)
        .bind(i.risk)
        .bind(&i.summary)
        .bind(sqlx::types::Json(if i.evidence.is_null() { json!([]) } else { i.evidence.clone() }))
        .bind(&i.containment)
        .bind(sqlx::types::Json(&i.notes))
        .bind(i.finding.as_ref().map(sqlx::types::Json))
        .bind(&i.model)
        .bind(i.input_tokens)
        .bind(i.output_tokens)
        .bind(i.started_at)
        .fetch_one(self.pool())
        .await?;
        from_row(&row)
    }

    /// Newest first.
    pub async fn investigations(&self, p: &ListParams) -> Result<Vec<Investigation>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT * FROM investigations
             WHERE ($1::text IS NULL OR status = $1)
               AND ($2::text IS NULL OR subject_type = $2)
               AND ($3::text IS NULL OR subject_id = $3)
               AND ($4::timestamptz IS NULL OR finished_at > $4)
             ORDER BY finished_at DESC LIMIT $5",
        )
        .bind(p.status.as_deref())
        .bind(p.subject_type.as_deref())
        .bind(p.subject_id.as_deref())
        .bind(p.since)
        .bind(p.limit.unwrap_or(50).clamp(1, MAX_LIST))
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(from_row).collect()
    }

    pub async fn investigation(&self, id: InvestigationId) -> Result<Option<Investigation>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM investigations WHERE id = $1")
            .bind(id.uuid())
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// Tokens spent on investigations finished after `since`.
    pub async fn investigation_usage(&self, since: DateTime<Utc>) -> Result<Usage, sqlx::Error> {
        let (investigations, input_tokens, output_tokens): (i64, i64, i64) = sqlx::query_as(
            "SELECT count(*), coalesce(sum(input_tokens), 0)::bigint, coalesce(sum(output_tokens), 0)::bigint
             FROM investigations WHERE finished_at > $1",
        )
        .bind(since)
        .fetch_one(self.pool())
        .await?;
        Ok(Usage { investigations, input_tokens, output_tokens })
    }
}

fn unavailable(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "investigations storage failed");
    (StatusCode::SERVICE_UNAVAILABLE, "storage error").into_response()
}

async fn put(State(store): State<ContextStore>, Json(input): Json<InvestigationInput>) -> Response {
    if let Err(e) = input.validate() {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    match store.put_investigation(&input).await {
        Ok(i) => (StatusCode::CREATED, Json(json!({ "investigation": i }))).into_response(),
        Err(e) => unavailable(e),
    }
}

async fn list(State(store): State<ContextStore>, Query(p): Query<ListParams>) -> Response {
    match store.investigations(&p).await {
        Ok(list) => Json(json!({ "investigations": list })).into_response(),
        Err(e) => unavailable(e),
    }
}

async fn one(State(store): State<ContextStore>, Path(qid): Path<String>) -> Response {
    let Ok(id) = qid.parse::<InvestigationId>() else {
        return (StatusCode::BAD_REQUEST, "not an investigation QID").into_response();
    };
    match store.investigation(id).await {
        Ok(Some(i)) => Json(json!({ "investigation": i })).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such investigation").into_response(),
        Err(e) => unavailable(e),
    }
}

#[derive(Deserialize)]
struct UsageParams {
    since: DateTime<Utc>,
}

async fn usage(State(store): State<ContextStore>, Query(p): Query<UsageParams>) -> Response {
    match store.investigation_usage(p.since).await {
        Ok(u) => Json(json!({ "usage": u })).into_response(),
        Err(e) => unavailable(e),
    }
}

/// `POST /v1/investigations`: the reasoner records one.
pub fn write_router(store: ContextStore) -> Router {
    Router::new().route("/v1/investigations", post(put)).with_state(store)
}

/// `GET /v1/investigations`, `GET /v1/investigations/{qid}`, and `GET /v1/investigations/usage`.
pub fn read_router(store: ContextStore) -> Router {
    Router::new()
        .route("/v1/investigations", get(list))
        .route("/v1/investigations/usage", get(usage))
        .route("/v1/investigations/{qid}", get(one))
        .with_state(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::test_store;

    fn input(id: &str, tokens: i64) -> InvestigationInput {
        InvestigationInput {
            subject: EntityRef::new("Human", id),
            trigger: "risk 63".into(),
            floor: 63,
            status: "needs_review".into(),
            verdict: "suspicious".into(),
            risk: 70,
            summary: "Read strategy's plans, then exported her accounts.".into(),
            evidence: json!([{ "id": "decision_1", "why": "the export" }]),
            containment: "none".into(),
            notes: vec!["kept at the floor".into()],
            finding: Some(json!({ "verdict": "suspicious" })),
            model: "test-model".into(),
            input_tokens: tokens,
            output_tokens: tokens / 10,
            started_at: Utc::now(),
        }
    }

    #[test]
    fn only_principals_and_known_values_are_stored() {
        assert!(input("nora", 10).validate().is_ok());
        assert!(InvestigationInput { subject: EntityRef::new("Resource", "doc"), ..input("x", 1) }.validate().is_err());
        assert!(InvestigationInput { verdict: "guilty".into(), ..input("x", 1) }.validate().is_err());
        assert!(InvestigationInput { risk: 101, ..input("x", 1) }.validate().is_err());
        assert!(InvestigationInput { input_tokens: -1, ..input("x", 1) }.validate().is_err());
    }

    #[tokio::test]
    async fn investigations_are_kept_listed_and_counted() {
        let Some(store) = test_store().await else { return };
        let before = Utc::now() - chrono::Duration::seconds(1);
        let nora = store.put_investigation(&input("nora", 1000)).await.unwrap();
        store.put_investigation(&InvestigationInput { status: "done".into(), ..input("ben", 500) }).await.unwrap();
        assert!(nora.qid.starts_with("investigation_"));

        let id: InvestigationId = nora.qid.parse().unwrap();
        assert_eq!(store.investigation(id).await.unwrap().unwrap().input, nora.input);
        let theirs =
            ListParams { subject_type: Some("Human".into()), subject_id: Some("nora".into()), ..Default::default() };
        assert_eq!(store.investigations(&theirs).await.unwrap().len(), 1);
        let review = ListParams { status: Some("needs_review".into()), ..Default::default() };
        assert!(store.investigations(&review).await.unwrap().iter().all(|i| i.input.status == "needs_review"));

        let usage = store.investigation_usage(before).await.unwrap();
        assert_eq!((usage.investigations, usage.input_tokens, usage.output_tokens), (2, 1500, 150));
    }
}

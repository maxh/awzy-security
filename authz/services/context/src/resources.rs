//! Resources and work items (demo plan step 2): what principals act on, and what they're working
//! on. Connectors write them through `/v1/resources` and `/v1/work-items`, one at a time or in
//! batches; the PDP reads a resource and its ancestors as Cedar entities at
//! `/v1/resources/{id}/entities` (demo plan step 3), with who is assigned to work on it (step 4,
//! ADR 0046).
//!
//! IDs are the connector's, stable across syncs (`synthetic:doc:00042`, a Drive file's
//! `gdrive:<id>`), and are the `Resource` entity IDs policies name.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};

use crate::store::{ContextStore, StoreError};

mod entities;

pub use entities::{chain_entities, entity};

/// Sensitivity tiers, lowest to highest, as the Cedar schema names them.
pub const SENSITIVITIES: &[&str] = &["public", "internal", "confidential", "crown_jewels"];

/// What a resource that hasn't been classified yet is treated as. Policies see it with
/// `unclassified` set, so they can challenge it (platform plan, "Unknown resources").
pub const DEFAULT_SENSITIVITY: &str = "internal";

/// How many parent folders up `/entities` follows. Deeper trees are cut off there, and a cycle
/// (which connectors can write: parents aren't foreign keys) ends at it too.
pub const MAX_ANCESTORS: i32 = 32;

/// The most items one batch request may carry.
pub const MAX_BATCH: usize = 1000;

const MAX_TEXT: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Share {
    /// human | group | domain | anyone
    pub kind: String,
    /// A userName, a group name, or a domain; empty for `anyone`.
    #[serde(default)]
    pub id: String,
    /// reader | commenter | writer | owner
    #[serde(default = "reader")]
    pub role: String,
}

fn reader() -> String {
    "reader".into()
}

/// A resource as a connector writes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ResourceInput {
    #[serde(default)]
    pub id: String,
    pub source: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub sensitivity: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub sharing: Vec<Share>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub attributes: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Resource {
    pub id: String,
    pub source: String,
    pub kind: String,
    pub name: String,
    pub parent: Option<String>,
    pub owner: Option<String>,
    pub sensitivity: Option<String>,
    pub labels: Vec<String>,
    pub sharing: Vec<Share>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub attributes: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// What awzy-classify decided, once it has looked at this version (ADR 0047).
    pub classification: Option<Classification>,
}

/// The classifier's result for a resource.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Classification {
    /// `None` when nothing placed it: it waits for the model step.
    pub sensitivity: Option<String>,
    /// label | content | metadata
    pub by: Option<String>,
    pub reason: String,
    pub at: DateTime<Utc>,
}

impl Resource {
    /// The tier policies see: the higher of the connector's and the classifier's, so neither a
    /// missing label nor a stale classification lowers it. `None` when neither has one.
    pub fn effective_sensitivity(&self) -> Option<&str> {
        let classified = self.classification.as_ref().and_then(|c| c.sensitivity.as_deref());
        let rank = |s: &str| SENSITIVITIES.iter().position(|t| *t == s);
        match (self.sensitivity.as_deref(), classified) {
            (Some(a), Some(b)) => Some(if rank(b) > rank(a) { b } else { a }),
            (a, b) => a.or(b),
        }
    }
}

/// A ticket, account, project, or statement of work: who is assigned, and which resources the
/// work needs.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkItemInput {
    #[serde(default)]
    pub id: String,
    pub source: String,
    pub kind: String,
    pub title: String,
    #[serde(default = "open")]
    pub state: String,
    #[serde(default)]
    pub assignees: Vec<String>,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default)]
    pub attributes: Value,
}

fn open() -> String {
    "open".into()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkItem {
    pub id: String,
    pub source: String,
    pub kind: String,
    pub title: String,
    pub state: String,
    pub assignees: Vec<String>,
    pub resources: Vec<String>,
    pub attributes: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn invalid(m: impl Into<String>) -> StoreError {
    StoreError::Invalid(m.into())
}

/// Entity IDs: 1 to 256 characters, letters, digits, and `. _ @ : / -`.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 256 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._@:/-".contains(&b))
}

/// A lowercase slug: `document`, `google-drive`, `statement_of_work`.
fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
}

impl ResourceInput {
    pub fn validate(&self) -> Result<(), StoreError> {
        if !valid_id(&self.id) {
            return Err(invalid(format!("resource ID {:?}: 1 to 256 letters, digits, and . _ @ : / -", self.id)));
        }
        if !valid_slug(&self.source) || !valid_slug(&self.kind) {
            return Err(invalid(format!("{}: source and kind are lowercase slugs", self.id)));
        }
        if self.name.trim().is_empty() || self.name.len() > 500 {
            return Err(invalid(format!("{}: the name is 1 to 500 characters", self.id)));
        }
        if self.parent.as_deref().is_some_and(|p| !valid_id(p) || p == self.id) {
            return Err(invalid(format!("{}: the parent must be another resource's ID", self.id)));
        }
        if let Some(s) = &self.sensitivity {
            if !SENSITIVITIES.contains(&s.as_str()) {
                return Err(invalid(format!("{}: sensitivity is one of {}", self.id, SENSITIVITIES.join(", "))));
            }
        }
        for share in &self.sharing {
            let kinds = ["human", "group", "domain", "anyone"];
            let roles = ["reader", "commenter", "writer", "owner"];
            if !kinds.contains(&share.kind.as_str()) || !roles.contains(&share.role.as_str()) {
                return Err(invalid(format!("{}: sharing kind or role {share:?} isn't known", self.id)));
            }
            if share.kind != "anyone" && share.id.trim().is_empty() {
                return Err(invalid(format!("{}: a {} share needs an ID", self.id, share.kind)));
            }
        }
        if self.text.as_ref().is_some_and(|t| t.len() > MAX_TEXT) {
            return Err(invalid(format!("{}: the text excerpt is at most {MAX_TEXT} bytes", self.id)));
        }
        if !(self.attributes.is_null() || self.attributes.is_object()) {
            return Err(invalid(format!("{}: attributes are a JSON object", self.id)));
        }
        Ok(())
    }
}

impl WorkItemInput {
    pub fn validate(&self) -> Result<(), StoreError> {
        if !valid_id(&self.id) {
            return Err(invalid(format!("work item ID {:?}: 1 to 256 letters, digits, and . _ @ : / -", self.id)));
        }
        if !valid_slug(&self.source) || !valid_slug(&self.kind) || !["open", "closed"].contains(&self.state.as_str()) {
            return Err(invalid(format!("{}: source and kind are lowercase slugs; state is open or closed", self.id)));
        }
        if self.title.trim().is_empty() || self.title.len() > 500 {
            return Err(invalid(format!("{}: the title is 1 to 500 characters", self.id)));
        }
        if self.assignees.iter().chain(&self.resources).any(|id| !valid_id(id)) {
            return Err(invalid(format!("{}: assignees and resources are entity IDs", self.id)));
        }
        if !(self.attributes.is_null() || self.attributes.is_object()) {
            return Err(invalid(format!("{}: attributes are a JSON object", self.id)));
        }
        Ok(())
    }
}

fn object(v: &Value) -> Value {
    if v.is_null() {
        json!({})
    } else {
        v.clone()
    }
}

const RESOURCE_COLUMNS: &str =
    "id, source, kind, name, parent, owner, sensitivity, labels, sharing, text, url, attributes, \
     created_at, updated_at, classified_sensitivity, classified_by, classification_reason, classified_at";

fn resource_from_row(row: &PgRow) -> Result<Resource, sqlx::Error> {
    Ok(Resource {
        id: row.try_get("id")?,
        source: row.try_get("source")?,
        kind: row.try_get("kind")?,
        name: row.try_get("name")?,
        parent: row.try_get("parent")?,
        owner: row.try_get("owner")?,
        sensitivity: row.try_get("sensitivity")?,
        labels: row.try_get("labels")?,
        sharing: row.try_get::<sqlx::types::Json<Vec<Share>>, _>("sharing")?.0,
        text: row.try_get("text")?,
        url: row.try_get("url")?,
        attributes: row.try_get::<sqlx::types::Json<Value>, _>("attributes")?.0,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        classification: row
            .try_get::<Option<DateTime<Utc>>, _>("classified_at")?
            .map(|at| {
                Ok::<_, sqlx::Error>(Classification {
                    sensitivity: row.try_get("classified_sensitivity")?,
                    by: row.try_get("classified_by")?,
                    reason: row.try_get::<Option<String>, _>("classification_reason")?.unwrap_or_default(),
                    at,
                })
            })
            .transpose()?,
    })
}

const WORK_ITEM_COLUMNS: &str =
    "id, source, kind, title, state, assignees, resources, attributes, created_at, updated_at";

fn work_item_from_row(row: &PgRow) -> Result<WorkItem, sqlx::Error> {
    Ok(WorkItem {
        id: row.try_get("id")?,
        source: row.try_get("source")?,
        kind: row.try_get("kind")?,
        title: row.try_get("title")?,
        state: row.try_get("state")?,
        assignees: row.try_get("assignees")?,
        resources: row.try_get("resources")?,
        attributes: row.try_get::<sqlx::types::Json<Value>, _>("attributes")?.0,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

async fn upsert_resource(tx: &mut Transaction<'_, Postgres>, r: &ResourceInput) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO resources (id, source, kind, name, parent, owner, sensitivity, labels, sharing, text, url, attributes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         ON CONFLICT (id) DO UPDATE SET source = $2, kind = $3, name = $4, parent = $5, owner = $6, sensitivity = $7,
           labels = $8, sharing = $9, text = $10, url = $11, attributes = $12, updated_at = now()",
    )
    .bind(&r.id)
    .bind(&r.source)
    .bind(&r.kind)
    .bind(&r.name)
    .bind(&r.parent)
    .bind(&r.owner)
    .bind(&r.sensitivity)
    .bind(&r.labels)
    .bind(sqlx::types::Json(&r.sharing))
    .bind(&r.text)
    .bind(&r.url)
    .bind(sqlx::types::Json(object(&r.attributes)))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_work_item(tx: &mut Transaction<'_, Postgres>, w: &WorkItemInput) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO work_items (id, source, kind, title, state, assignees, resources, attributes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (id) DO UPDATE SET source = $2, kind = $3, title = $4, state = $5, assignees = $6,
           resources = $7, attributes = $8, updated_at = now()",
    )
    .bind(&w.id)
    .bind(&w.source)
    .bind(&w.kind)
    .bind(&w.title)
    .bind(&w.state)
    .bind(&w.assignees)
    .bind(&w.resources)
    .bind(sqlx::types::Json(object(&w.attributes)))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Filters for listing resources, all optional. Pages are ordered by ID; `after` continues one.
#[derive(Debug, Default, Deserialize)]
pub struct ResourceFilter {
    /// Matches the resource's ID or name, ignoring case: the client portal's search.
    pub q: Option<String>,
    pub source: Option<String>,
    pub kind: Option<String>,
    pub owner: Option<String>,
    pub parent: Option<String>,
    pub after: Option<String>,
    pub limit: Option<i64>,
}

/// Filters for listing work items: by assignee, by a resource the work needs, or by the source
/// that wrote them (a connector finding what it no longer sees, ADR 0070).
#[derive(Debug, Default, Deserialize)]
pub struct WorkItemFilter {
    pub assignee: Option<String>,
    pub resource: Option<String>,
    pub source: Option<String>,
    pub after: Option<String>,
    pub limit: Option<i64>,
}

const MAX_PAGE: i64 = 500;

impl ContextStore {
    /// Writes resources, creating or replacing each, in one transaction: all or none.
    pub async fn put_resources(&self, items: &[ResourceInput]) -> Result<usize, StoreError> {
        if items.len() > MAX_BATCH {
            return Err(invalid(format!("at most {MAX_BATCH} resources per request")));
        }
        items.iter().try_for_each(ResourceInput::validate)?;
        let mut tx = self.pool().begin().await?;
        for r in items {
            upsert_resource(&mut tx, r).await?;
        }
        tx.commit().await?;
        Ok(items.len())
    }

    pub async fn resource(&self, id: &str) -> Result<Option<Resource>, sqlx::Error> {
        sqlx::query(&format!("SELECT {RESOURCE_COLUMNS} FROM resources WHERE id = $1"))
            .bind(id)
            .fetch_optional(self.pool())
            .await?
            .as_ref()
            .map(resource_from_row)
            .transpose()
    }

    /// The resource and its ancestors, nearest first; empty if there is no such resource. A
    /// parent that hasn't been written yet ends the chain.
    pub async fn resource_with_ancestors(&self, id: &str) -> Result<Vec<Resource>, sqlx::Error> {
        let prefixed: Vec<String> = RESOURCE_COLUMNS.split(", ").map(|c| format!("r.{c}")).collect();
        let rows = sqlx::query(&format!(
            "WITH RECURSIVE chain AS (
                 SELECT {RESOURCE_COLUMNS}, 0 AS depth FROM resources WHERE id = $1
                 UNION ALL
                 SELECT {}, c.depth + 1 FROM resources r JOIN chain c ON r.id = c.parent WHERE c.depth < $2
             )
             SELECT {RESOURCE_COLUMNS} FROM chain ORDER BY depth",
            prefixed.join(", ")
        ))
        .bind(id)
        .bind(MAX_ANCESTORS)
        .fetch_all(self.pool())
        .await?;
        let mut seen = std::collections::HashSet::new();
        let mut chain = Vec::new();
        for row in &rows {
            let r = resource_from_row(row)?;
            if seen.insert(r.id.clone()) {
                chain.push(r);
            }
        }
        Ok(chain)
    }

    /// The open work items that link any of `resources` (a resource and its ancestors, so work
    /// on a folder covers what's in it), each with who is assigned to it, by ID.
    pub async fn open_work_on(&self, resources: &[String]) -> Result<Vec<(String, Vec<String>)>, sqlx::Error> {
        sqlx::query_as("SELECT id, assignees FROM work_items WHERE state = 'open' AND resources && $1 ORDER BY id")
            .bind(resources)
            .fetch_all(self.pool())
            .await
    }

    /// Resources never classified, or changed since, oldest change first (ADR 0047).
    /// Claims up to `limit` resources to classify, locked for `conn`'s transaction. Rows another
    /// replica has claimed are skipped, so replicas share the queue instead of each doing all of it.
    pub async fn claim_resources_to_classify(
        &self,
        conn: &mut sqlx::PgConnection,
        limit: i64,
    ) -> Result<Vec<Resource>, sqlx::Error> {
        sqlx::query(&format!(
            "SELECT {RESOURCE_COLUMNS} FROM resources
             WHERE classified_at IS NULL OR classified_at < updated_at
             ORDER BY updated_at LIMIT $1
             FOR UPDATE SKIP LOCKED"
        ))
        .bind(limit)
        .fetch_all(conn)
        .await?
        .iter()
        .map(resource_from_row)
        .collect()
    }

    /// Records the classifier's result for the version of `r` it read. A write since then wins:
    /// the resource stays queued and is classified again.
    pub async fn set_classification(
        &self,
        conn: &mut sqlx::PgConnection,
        r: &Resource,
        result: Option<&awzy_classify::Classification>,
    ) -> Result<(), sqlx::Error> {
        let reason = result.map_or_else(
            || "nothing in its label, content, name, or folders decides it".to_string(),
            |c| c.reason.clone(),
        );
        sqlx::query(
            "UPDATE resources SET classified_sensitivity = $2, classified_by = $3, classification_reason = $4,
               classified_at = now()
             WHERE id = $1 AND updated_at = $5",
        )
        .bind(&r.id)
        .bind(result.map(|c| c.tier.as_str()))
        .bind(result.map(|c| c.source.as_str()))
        .bind(reason)
        .bind(r.updated_at)
        .execute(conn)
        .await?;
        Ok(())
    }

    pub async fn resources(&self, f: &ResourceFilter) -> Result<Vec<Resource>, sqlx::Error> {
        let rows = sqlx::query(&format!(
            "SELECT {RESOURCE_COLUMNS} FROM resources
             WHERE ($1::text IS NULL OR source = $1) AND ($2::text IS NULL OR kind = $2)
               AND ($3::text IS NULL OR owner = $3) AND ($4::text IS NULL OR parent = $4)
               AND id > $5
               AND ($7::text IS NULL OR id ILIKE $7 OR name ILIKE $7)
             ORDER BY id LIMIT $6"
        ))
        .bind(&f.source)
        .bind(&f.kind)
        .bind(&f.owner)
        .bind(&f.parent)
        .bind(f.after.as_deref().unwrap_or(""))
        .bind(f.limit.unwrap_or(100).clamp(1, MAX_PAGE))
        .bind(f.q.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(|q| format!("%{}%", like_escape(q))))
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(resource_from_row).collect()
    }

    pub async fn delete_resource(&self, id: &str) -> Result<bool, sqlx::Error> {
        let done = sqlx::query("DELETE FROM resources WHERE id = $1").bind(id).execute(self.pool()).await?;
        Ok(done.rows_affected() > 0)
    }

    /// Writes work items, creating or replacing each, in one transaction: all or none.
    pub async fn put_work_items(&self, items: &[WorkItemInput]) -> Result<usize, StoreError> {
        if items.len() > MAX_BATCH {
            return Err(invalid(format!("at most {MAX_BATCH} work items per request")));
        }
        items.iter().try_for_each(WorkItemInput::validate)?;
        let mut tx = self.pool().begin().await?;
        for w in items {
            upsert_work_item(&mut tx, w).await?;
        }
        tx.commit().await?;
        Ok(items.len())
    }

    pub async fn work_item(&self, id: &str) -> Result<Option<WorkItem>, sqlx::Error> {
        sqlx::query(&format!("SELECT {WORK_ITEM_COLUMNS} FROM work_items WHERE id = $1"))
            .bind(id)
            .fetch_optional(self.pool())
            .await?
            .as_ref()
            .map(work_item_from_row)
            .transpose()
    }

    pub async fn work_items(&self, f: &WorkItemFilter) -> Result<Vec<WorkItem>, sqlx::Error> {
        let rows = sqlx::query(&format!(
            "SELECT {WORK_ITEM_COLUMNS} FROM work_items
             WHERE ($1::text IS NULL OR $1 = ANY(assignees)) AND ($2::text IS NULL OR $2 = ANY(resources))
               AND ($5::text IS NULL OR source = $5) AND id > $3
             ORDER BY id LIMIT $4"
        ))
        .bind(&f.assignee)
        .bind(&f.resource)
        .bind(f.after.as_deref().unwrap_or(""))
        .bind(f.limit.unwrap_or(100).clamp(1, MAX_PAGE))
        .bind(&f.source)
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(work_item_from_row).collect()
    }

    pub async fn delete_work_item(&self, id: &str) -> Result<bool, sqlx::Error> {
        let done = sqlx::query("DELETE FROM work_items WHERE id = $1").bind(id).execute(self.pool()).await?;
        Ok(done.rows_affected() > 0)
    }
}

// ---- HTTP

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!(error = %e, "awzy-context request failed");
    (StatusCode::SERVICE_UNAVAILABLE, "storage error").into_response()
}

fn store_error(e: StoreError) -> Response {
    match e {
        StoreError::Invalid(m) | StoreError::Conflict(m) => (StatusCode::BAD_REQUEST, m).into_response(),
        StoreError::Db(e) => internal(e),
    }
}

async fn put_resource(
    State(store): State<ContextStore>,
    Path(id): Path<String>,
    Json(mut input): Json<ResourceInput>,
) -> Response {
    input.id = id;
    match store.put_resources(std::slice::from_ref(&input)).await {
        Ok(_) => match store.resource(&input.id).await {
            Ok(Some(r)) => Json(json!({ "resource": r })).into_response(),
            Ok(None) => internal("resource vanished after writing"),
            Err(e) => internal(e),
        },
        Err(e) => store_error(e),
    }
}

#[derive(Deserialize)]
struct ResourceBatch {
    resources: Vec<ResourceInput>,
}

async fn put_resource_batch(State(store): State<ContextStore>, Json(batch): Json<ResourceBatch>) -> Response {
    match store.put_resources(&batch.resources).await {
        Ok(n) => Json(json!({ "written": n })).into_response(),
        Err(e) => store_error(e),
    }
}

async fn get_resource(State(store): State<ContextStore>, Path(id): Path<String>) -> Response {
    match store.resource(&id).await {
        Ok(Some(r)) => Json(json!({ "resource": r })).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such resource").into_response(),
        Err(e) => internal(e),
    }
}

async fn get_resource_entity(State(store): State<ContextStore>, Path(id): Path<String>) -> Response {
    match store.resource(&id).await {
        Ok(Some(r)) => Json(entity(&r)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such resource").into_response(),
        Err(e) => internal(e),
    }
}

/// The resource and its ancestors as Cedar entities, nearest first, who is assigned to work on
/// it, and the open work items that link it with their assignees (ADR 0118): what the PDP loads.
async fn get_resource_entities(State(store): State<ContextStore>, Path(id): Path<String>) -> Response {
    let chain = match store.resource_with_ancestors(&id).await {
        Ok(chain) if chain.is_empty() => return (StatusCode::NOT_FOUND, "no such resource").into_response(),
        Ok(chain) => chain,
        Err(e) => return internal(e),
    };
    let ids: Vec<String> = chain.iter().map(|r| r.id.clone()).collect();
    match store.open_work_on(&ids).await {
        Ok(work) => {
            let assignees: std::collections::BTreeSet<&String> = work.iter().flat_map(|(_, a)| a).collect();
            let work: Vec<Value> = work.iter().map(|(id, a)| json!({ "id": id, "assignees": a })).collect();
            Json(json!({ "entities": chain_entities(&chain), "assignees": assignees, "work": work })).into_response()
        }
        Err(e) => internal(e),
    }
}

/// `after`: the cursor from the last answer; absent on a caller's first call, which gets the
/// cursor to start from. `wait`: seconds to wait for a change before answering empty (ADR 0077).
#[derive(Debug, Default, Deserialize)]
pub struct ChangesParams {
    pub after: Option<i64>,
    pub wait: Option<u64>,
}

/// The most changes one answer carries; a caller with more to read asks again at once.
const MAX_CHANGES: i64 = 10_000;

/// Which resources' entities may have changed since `after`, so a PDP drops them from its cache.
/// `reset` means the log no longer reaches back that far: drop everything.
async fn resource_changes(State(store): State<ContextStore>, Query(p): Query<ChangesParams>) -> Response {
    let read = |store: ContextStore| async move { store.resource_changes(p.after, MAX_CHANGES).await };
    // Taken before reading, so a change committed in between still wakes the wait.
    let seen = store.resources_changed().current();
    let mut answer = read(store.clone()).await;
    if let Ok((_, ids, false)) = &answer {
        if ids.is_empty() && p.after.is_some() && p.wait.unwrap_or(0) > 0 {
            let after = p.after;
            let check = || {
                let store = store.clone();
                async move {
                    store.resource_changes(after, 1).await.map_or(true, |(_, ids, reset)| reset || !ids.is_empty())
                }
            };
            store.resources_changed().wait_until(seen, awzy_store::changes::wait_for(p.wait), check).await;
            answer = read(store.clone()).await;
        }
    }
    match answer {
        Ok((cursor, ids, reset)) => Json(json!({ "cursor": cursor, "ids": ids, "reset": reset })).into_response(),
        Err(e) => internal(e),
    }
}

async fn list_resources(State(store): State<ContextStore>, Query(f): Query<ResourceFilter>) -> Response {
    match store.resources(&f).await {
        Ok(list) => Json(json!({ "resources": list })).into_response(),
        Err(e) => internal(e),
    }
}

async fn delete_resource(State(store): State<ContextStore>, Path(id): Path<String>) -> Response {
    match store.delete_resource(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "no such resource").into_response(),
        Err(e) => internal(e),
    }
}

async fn put_work_item(
    State(store): State<ContextStore>,
    Path(id): Path<String>,
    Json(mut input): Json<WorkItemInput>,
) -> Response {
    input.id = id;
    match store.put_work_items(std::slice::from_ref(&input)).await {
        Ok(_) => match store.work_item(&input.id).await {
            Ok(Some(w)) => Json(json!({ "work_item": w })).into_response(),
            Ok(None) => internal("work item vanished after writing"),
            Err(e) => internal(e),
        },
        Err(e) => store_error(e),
    }
}

#[derive(Deserialize)]
struct WorkItemBatch {
    work_items: Vec<WorkItemInput>,
}

async fn put_work_item_batch(State(store): State<ContextStore>, Json(batch): Json<WorkItemBatch>) -> Response {
    match store.put_work_items(&batch.work_items).await {
        Ok(n) => Json(json!({ "written": n })).into_response(),
        Err(e) => store_error(e),
    }
}

async fn get_work_item(State(store): State<ContextStore>, Path(id): Path<String>) -> Response {
    match store.work_item(&id).await {
        Ok(Some(w)) => Json(json!({ "work_item": w })).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such work item").into_response(),
        Err(e) => internal(e),
    }
}

async fn list_work_items(State(store): State<ContextStore>, Query(f): Query<WorkItemFilter>) -> Response {
    match store.work_items(&f).await {
        Ok(list) => Json(json!({ "work_items": list })).into_response(),
        Err(e) => internal(e),
    }
}

async fn delete_work_item(State(store): State<ContextStore>, Path(id): Path<String>) -> Response {
    match store.delete_work_item(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "no such work item").into_response(),
        Err(e) => internal(e),
    }
}

/// `/v1/resources` and `/v1/work-items` for connectors (writes, in batches of up to
/// [`MAX_BATCH`]) and for enforcement points and the PDP (reads).
/// Callers authenticate with service tokens (ADR 0055; see `Access`).
/// Reading resources and work items: what the PDP and connectors need (ADR 0055 groups them).
pub fn read_router(store: ContextStore) -> Router {
    Router::new()
        .route("/v1/resources", get(list_resources))
        .route("/v1/resources/changes", get(resource_changes))
        .route("/v1/resources/{id}", get(get_resource))
        .route("/v1/resources/{id}/entity", get(get_resource_entity))
        .route("/v1/resources/{id}/entities", get(get_resource_entities))
        .route("/v1/work-items", get(list_work_items))
        .route("/v1/work-items/{id}", get(get_work_item))
        .with_state(store)
}

/// Writing resources and work items: what connectors do.
pub fn write_router(store: ContextStore) -> Router {
    Router::new()
        .route("/v1/resources/batch", post(put_resource_batch))
        .route("/v1/resources/{id}", put(put_resource).delete(delete_resource))
        .route("/v1/work-items/batch", post(put_work_item_batch))
        .route("/v1/work-items/{id}", put(put_work_item).delete(delete_work_item))
        .with_state(store)
}

/// `q` with LIKE's wildcards taken literally.
fn like_escape(q: &str) -> String {
    q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Both, unguarded: for tests.
#[cfg(test)]
pub fn router(store: ContextStore) -> Router {
    read_router(store.clone()).merge(write_router(store))
}

#[cfg(test)]
mod tests;

//! SCIM 2.0 (RFC 7643, 7644) for the client's IdP to provision people and groups (ADR 0031).
//!
//! Okta, Entra ID, and Google Workspace push users and groups here as they change, so awzy
//! holds no IdP admin credentials and deprovisioning arrives in seconds. Covers what those IdPs
//! use: Users and Groups with create, read, list with an `eq` filter, replace, PATCH, and delete,
//! plus ServiceProviderConfig. Tolerates their dialects: Entra sends `"Replace"` and booleans
//! as `"False"`, and removes group members either by filter path or by value list.
//!
//! The IdP authenticates with a bearer token the client generates and sets in both places
//! (AWZY_SCIM_TOKEN).

// Request helpers return the finished SCIM error `Response` as their error, at most once per
// request, so its size doesn't matter.
#![expect(clippy::result_large_err, reason = "errors are finished responses, built once")]

use axum::{
    body::Bytes,
    extract::{Path, Query, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    human_attributes::ENTERPRISE_SCHEMA,
    store::{ContextStore, Group, GroupInput, StoreError, User, UserInput},
};

const USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
const GROUP_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
const LIST_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
const ERROR_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:Error";
const BASE: &str = "/scim/v2";
const MAX_PAGE: i64 = 200;

#[derive(Clone)]
struct Scim {
    store: ContextStore,
    /// SHA-256 of the IdP's bearer token, compared in constant time.
    token_sha256: [u8; 32],
}

/// `/scim/v2/...`, behind the IdP's bearer token.
pub fn router(store: ContextStore, token: &str) -> Router {
    let state = Scim { store, token_sha256: Sha256::digest(token.as_bytes()).into() };
    Router::new()
        .route(&format!("{BASE}/Users"), get(list_users).post(create_user))
        .route(&format!("{BASE}/Users/{{id}}"), get(get_user).put(replace_user).patch(patch_user).delete(delete_user))
        .route(&format!("{BASE}/Groups"), get(list_groups).post(create_group))
        .route(
            &format!("{BASE}/Groups/{{id}}"),
            get(get_group).put(replace_group).patch(patch_group).delete(delete_group),
        )
        .route(&format!("{BASE}/ServiceProviderConfig"), get(service_provider_config))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state)
}

async fn authenticate(State(scim): State<Scim>, req: Request, next: Next) -> Response {
    let presented =
        req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
    let ok = presented.is_some_and(|t| {
        let digest: [u8; 32] = Sha256::digest(t.as_bytes()).into();
        digest.iter().zip(scim.token_sha256.iter()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    });
    if !ok {
        return error(StatusCode::UNAUTHORIZED, None, "a valid SCIM bearer token is required");
    }
    next.run(req).await
}

// ---- Responses

fn scim(status: StatusCode, body: Value) -> Response {
    (status, [(header::CONTENT_TYPE, "application/scim+json")], body.to_string()).into_response()
}

fn error(status: StatusCode, scim_type: Option<&str>, detail: impl std::fmt::Display) -> Response {
    let mut body =
        json!({ "schemas": [ERROR_SCHEMA], "status": status.as_u16().to_string(), "detail": detail.to_string() });
    if let Some(t) = scim_type {
        body["scimType"] = json!(t);
    }
    scim(status, body)
}

fn not_found(what: &str) -> Response {
    error(StatusCode::NOT_FOUND, None, format!("no {what} with that id"))
}

fn store_error(e: StoreError) -> Response {
    match e {
        StoreError::Conflict(m) => error(StatusCode::CONFLICT, Some("uniqueness"), m),
        StoreError::Invalid(m) => error(StatusCode::BAD_REQUEST, Some("invalidValue"), m),
        StoreError::Db(e) => db_error(e),
    }
}

fn db_error(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "SCIM: database error");
    error(StatusCode::INTERNAL_SERVER_ERROR, None, "storage error")
}

fn meta(kind: &str, id: Uuid, created: chrono::DateTime<chrono::Utc>, updated: chrono::DateTime<chrono::Utc>) -> Value {
    json!({ "resourceType": kind, "created": created, "lastModified": updated, "location": format!("{BASE}/{kind}s/{id}") })
}

fn user_json(u: &User) -> Value {
    let mut r = u.input.resource.as_object().cloned().unwrap_or_default();
    r.remove("password");
    let schemas = if r.contains_key(ENTERPRISE_SCHEMA) { json!([USER_SCHEMA, ENTERPRISE_SCHEMA]) } else { json!([USER_SCHEMA]) };
    r.insert("schemas".into(), schemas);
    r.insert("id".into(), json!(u.scim_id));
    r.insert("userName".into(), json!(u.input.user_name));
    r.insert("active".into(), json!(u.input.active));
    for (k, v) in [("externalId", &u.input.external_id), ("displayName", &u.input.display_name)] {
        match v {
            Some(v) => r.insert(k.into(), json!(v)),
            None => r.remove(k),
        };
    }
    r.insert("meta".into(), meta("User", u.scim_id, u.created_at, u.updated_at));
    Value::Object(r)
}

fn group_json(g: &Group) -> Value {
    let members: Vec<Value> = g
        .members
        .iter()
        .map(|m| json!({ "value": m.id, "display": m.display, "type": m.kind, "$ref": format!("{BASE}/{}s/{}", m.kind, m.id) }))
        .collect();
    let mut r = json!({
        "schemas": [GROUP_SCHEMA], "id": g.scim_id, "displayName": g.input.display_name,
        "members": members, "meta": meta("Group", g.scim_id, g.created_at, g.updated_at),
    });
    if let Some(e) = &g.input.external_id {
        r["externalId"] = json!(e);
    }
    r
}

fn list(total: i64, start: i64, resources: Vec<Value>) -> Response {
    scim(
        StatusCode::OK,
        json!({ "schemas": [LIST_SCHEMA], "totalResults": total, "startIndex": start, "itemsPerPage": resources.len(), "Resources": resources }),
    )
}

// ---- Requests

fn body_json(body: &Bytes) -> Result<Map<String, Value>, Response> {
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(m)) => Ok(m),
        _ => Err(error(StatusCode::BAD_REQUEST, Some("invalidSyntax"), "the body must be a JSON object")),
    }
}

/// SCIM booleans, including Entra's strings ("True", "False").
fn as_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::String(s) => s.parse::<bool>().ok().or_else(|| s.to_ascii_lowercase().parse().ok()),
        _ => None,
    }
}

fn as_string(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

/// The user's display name: displayName, else name.formatted.
fn display_name(r: &Map<String, Value>) -> Option<String> {
    r.get("displayName")
        .and_then(as_string)
        .or_else(|| r.get("name").and_then(|n| n.get("formatted")).and_then(as_string))
}

fn user_input(mut r: Map<String, Value>) -> Result<UserInput, Response> {
    for k in ["password", "id", "meta", "schemas"] {
        r.remove(k);
    }
    let user_name = r.get("userName").and_then(as_string).filter(|u| !u.trim().is_empty());
    let Some(user_name) = user_name else {
        return Err(error(StatusCode::BAD_REQUEST, Some("invalidValue"), "userName is required"));
    };
    Ok(UserInput {
        user_name,
        external_id: r.get("externalId").and_then(as_string),
        display_name: display_name(&r),
        active: r.get("active").and_then(as_bool).unwrap_or(true),
        resource: Value::Object(r),
    })
}

#[derive(Deserialize)]
struct ListParams {
    filter: Option<String>,
    #[serde(rename = "startIndex")]
    start_index: Option<i64>,
    count: Option<i64>,
}

/// `<attribute> eq "<value>"` on `attribute` (case-insensitive), the only filter IdPs need here.
fn eq_filter(filter: &Option<String>, attribute: &str) -> Result<Option<String>, Response> {
    let Some(f) = filter.as_deref().map(str::trim).filter(|f| !f.is_empty()) else { return Ok(None) };
    let parsed = f.split_once(char::is_whitespace).and_then(|(attr, rest)| {
        let (op, value) = rest.trim_start().split_once(char::is_whitespace)?;
        let value = value.trim().strip_prefix('"')?.strip_suffix('"')?;
        (attr.eq_ignore_ascii_case(attribute) && op.eq_ignore_ascii_case("eq")).then(|| value.replace("\\\"", "\""))
    });
    parsed.map(Some).ok_or_else(|| {
        error(StatusCode::BAD_REQUEST, Some("invalidFilter"), format!("only `{attribute} eq \"…\"` is supported"))
    })
}

fn paging(p: &ListParams) -> (i64, i64) {
    let start = p.start_index.unwrap_or(1).max(1);
    (start, p.count.unwrap_or(100).clamp(0, MAX_PAGE))
}

#[derive(Deserialize)]
struct PatchRequest {
    #[serde(rename = "Operations", alias = "operations")]
    operations: Vec<PatchOp>,
}

#[derive(Deserialize)]
struct PatchOp {
    op: String,
    path: Option<String>,
    value: Option<Value>,
}

fn patch_ops(body: &Bytes) -> Result<Vec<PatchOp>, Response> {
    serde_json::from_slice::<PatchRequest>(body)
        .map(|p| p.operations)
        .map_err(|e| error(StatusCode::BAD_REQUEST, Some("invalidSyntax"), format!("not a PatchOp: {e}")))
}

// ---- Users

async fn list_users(State(s): State<Scim>, Query(p): Query<ListParams>) -> Response {
    let user_name = match eq_filter(&p.filter, "userName") {
        Ok(f) => f,
        Err(r) => return r,
    };
    let (start, count) = paging(&p);
    match s.store.find_users(user_name.as_deref(), start - 1, count).await {
        Ok((total, users)) => list(total, start, users.iter().map(user_json).collect()),
        Err(e) => db_error(e),
    }
}

async fn create_user(State(s): State<Scim>, body: Bytes) -> Response {
    let input = match body_json(&body).and_then(user_input) {
        Ok(i) => i,
        Err(r) => return r,
    };
    match s.store.create_user(&input).await {
        Ok(u) => {
            tracing::info!(user = %u.input.user_name, "SCIM: user provisioned");
            scim(StatusCode::CREATED, user_json(&u))
        }
        Err(e) => store_error(e),
    }
}

async fn get_user(State(s): State<Scim>, Path(id): Path<Uuid>) -> Response {
    match s.store.get_user(id).await {
        Ok(Some(u)) => scim(StatusCode::OK, user_json(&u)),
        Ok(None) => not_found("user"),
        Err(e) => db_error(e),
    }
}

async fn replace_user(State(s): State<Scim>, Path(id): Path<Uuid>, body: Bytes) -> Response {
    let input = match body_json(&body).and_then(user_input) {
        Ok(i) => i,
        Err(r) => return r,
    };
    match s.store.replace_user(id, &input).await {
        Ok(Some(u)) => scim(StatusCode::OK, user_json(&u)),
        Ok(None) => not_found("user"),
        Err(e) => store_error(e),
    }
}

/// Sets one attribute of a user's resource. Filtered paths (`emails[type eq "work"].value`) are
/// ignored: awzy doesn't use those attributes.
fn set_attribute(r: &mut Map<String, Value>, path: &str, value: Value) {
    if path.contains('[') {
        return;
    }
    // The enterprise extension's attributes (Entra ID: `urn:…:enterprise:2.0:User:department`),
    // whose URN holds a `.` that isn't a sub-attribute.
    if let Some(rest) = path.strip_prefix(ENTERPRISE_SCHEMA) {
        match rest.strip_prefix(':') {
            Some(attr) => {
                let entry = r.entry(ENTERPRISE_SCHEMA.to_string()).or_insert_with(|| json!({}));
                if !entry.is_object() {
                    *entry = json!({});
                }
                if let Some(ext) = entry.as_object_mut() {
                    set_attribute(ext, attr, value);
                }
            }
            None if value.is_null() => {
                r.remove(ENTERPRISE_SCHEMA);
            }
            None => {
                r.insert(ENTERPRISE_SCHEMA.to_string(), value);
            }
        }
        return;
    }
    let path = path.strip_prefix(&format!("{USER_SCHEMA}:")).unwrap_or(path);
    let known = ["userName", "active", "externalId", "displayName"];
    let key = known.iter().find(|k| k.eq_ignore_ascii_case(path)).map_or(path, |k| *k);
    let value = if key == "active" { as_bool(&value).map_or(value, Value::Bool) } else { value };
    match key.split_once('.') {
        Some((outer, inner)) => {
            let entry = r.entry(outer.to_string()).or_insert_with(|| json!({}));
            if let Some(obj) = entry.as_object_mut() {
                obj.insert(inner.to_string(), value);
            }
        }
        None if value.is_null() => {
            r.remove(key);
        }
        None => {
            r.insert(key.to_string(), value);
        }
    }
}

async fn patch_user(State(s): State<Scim>, Path(id): Path<Uuid>, body: Bytes) -> Response {
    let ops = match patch_ops(&body) {
        Ok(o) => o,
        Err(r) => return r,
    };
    let user = match s.store.get_user(id).await {
        Ok(Some(u)) => u,
        Ok(None) => return not_found("user"),
        Err(e) => return db_error(e),
    };
    let mut r = user_json(&user).as_object().cloned().unwrap_or_default();
    for op in ops {
        let remove = op.op.eq_ignore_ascii_case("remove");
        match (op.path, op.value) {
            (Some(path), _) if remove => set_attribute(&mut r, &path, Value::Null),
            (Some(path), Some(value)) => set_attribute(&mut r, &path, value),
            (None, Some(Value::Object(values))) => {
                for (k, v) in values {
                    set_attribute(&mut r, &k, v);
                }
            }
            _ => {
                return error(
                    StatusCode::BAD_REQUEST,
                    Some("invalidSyntax"),
                    "each operation needs a path or an object value",
                )
            }
        }
    }
    let input = match user_input(r) {
        Ok(i) => i,
        Err(r) => return r,
    };
    if input.active != user.input.active {
        tracing::info!(user = %input.user_name, active = input.active, "SCIM: user (de)activated");
    }
    match s.store.replace_user(id, &input).await {
        Ok(Some(u)) => scim(StatusCode::OK, user_json(&u)),
        Ok(None) => not_found("user"),
        Err(e) => store_error(e),
    }
}

async fn delete_user(State(s): State<Scim>, Path(id): Path<Uuid>) -> Response {
    match s.store.delete_user(id).await {
        Ok(true) => {
            tracing::info!(%id, "SCIM: user deleted");
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => not_found("user"),
        Err(e) => db_error(e),
    }
}

// ---- Groups

fn member_ids(v: &Value) -> Vec<Uuid> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("value").and_then(Value::as_str))
        .filter_map(|s| s.parse().ok())
        .collect()
}

fn group_input(r: &Map<String, Value>) -> Result<GroupInput, Response> {
    match r.get("displayName").and_then(as_string).filter(|d| !d.trim().is_empty()) {
        Some(display_name) => Ok(GroupInput { display_name, external_id: r.get("externalId").and_then(as_string) }),
        None => Err(error(StatusCode::BAD_REQUEST, Some("invalidValue"), "displayName is required")),
    }
}

async fn list_groups(State(s): State<Scim>, Query(p): Query<ListParams>) -> Response {
    let name = match eq_filter(&p.filter, "displayName") {
        Ok(f) => f,
        Err(r) => return r,
    };
    let (start, count) = paging(&p);
    match s.store.find_groups(name.as_deref(), start - 1, count).await {
        Ok((total, groups)) => list(total, start, groups.iter().map(group_json).collect()),
        Err(e) => db_error(e),
    }
}

async fn create_group(State(s): State<Scim>, body: Bytes) -> Response {
    let r = match body_json(&body) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let input = match group_input(&r) {
        Ok(i) => i,
        Err(e) => return e,
    };
    let members = r.get("members").map(member_ids).unwrap_or_default();
    match s.store.create_group(&input, &members).await {
        Ok(g) => {
            tracing::info!(group = %g.input.display_name, members = g.members.len(), "SCIM: group provisioned");
            scim(StatusCode::CREATED, group_json(&g))
        }
        Err(e) => store_error(e),
    }
}

async fn get_group(State(s): State<Scim>, Path(id): Path<Uuid>) -> Response {
    match s.store.get_group(id).await {
        Ok(Some(g)) => scim(StatusCode::OK, group_json(&g)),
        Ok(None) => not_found("group"),
        Err(e) => db_error(e),
    }
}

async fn replace_group(State(s): State<Scim>, Path(id): Path<Uuid>, body: Bytes) -> Response {
    let r = match body_json(&body) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let input = match group_input(&r) {
        Ok(i) => i,
        Err(e) => return e,
    };
    let members = r.get("members").map(member_ids).unwrap_or_default();
    match s.store.update_group(id, Some(&input), Some(&members), &[], &[]).await {
        Ok(Some(g)) => scim(StatusCode::OK, group_json(&g)),
        Ok(None) => not_found("group"),
        Err(e) => store_error(e),
    }
}

/// `members[value eq "<id>"]`
fn member_filter(path: &str) -> Option<Uuid> {
    let inner = path.strip_prefix("members[")?.strip_suffix(']')?;
    let (attr, rest) = inner.split_once(char::is_whitespace)?;
    let (op, value) = rest.trim_start().split_once(char::is_whitespace)?;
    (attr.eq_ignore_ascii_case("value") && op.eq_ignore_ascii_case("eq")).then_some(())?;
    value.trim().trim_matches('"').parse().ok()
}

async fn patch_group(State(s): State<Scim>, Path(id): Path<Uuid>, body: Bytes) -> Response {
    let ops = match patch_ops(&body) {
        Ok(o) => o,
        Err(r) => return r,
    };
    let group = match s.store.get_group(id).await {
        Ok(Some(g)) => g,
        Ok(None) => return not_found("group"),
        Err(e) => return db_error(e),
    };
    let mut input = group.input.clone();
    let (mut set, mut add, mut remove): (Option<Vec<Uuid>>, Vec<Uuid>, Vec<Uuid>) = (None, vec![], vec![]);
    for op in ops {
        let kind = op.op.to_ascii_lowercase();
        let path = op.path.as_deref().map(str::trim);
        match (kind.as_str(), path, op.value) {
            ("add", Some(p), Some(v)) if p.eq_ignore_ascii_case("members") => add.extend(member_ids(&v)),
            ("replace", Some(p), Some(v)) if p.eq_ignore_ascii_case("members") => set = Some(member_ids(&v)),
            ("remove", Some(p), Some(v)) if p.eq_ignore_ascii_case("members") => remove.extend(member_ids(&v)),
            ("remove", Some(p), None) if p.eq_ignore_ascii_case("members") => set = Some(vec![]),
            ("remove", Some(p), _) if member_filter(p).is_some() => remove.extend(member_filter(p)),
            ("replace" | "add", Some(p), Some(v)) if p.eq_ignore_ascii_case("displayName") => {
                if let Some(name) = as_string(&v) {
                    input.display_name = name;
                }
            }
            ("replace" | "add", Some(p), Some(v)) if p.eq_ignore_ascii_case("externalId") => {
                input.external_id = as_string(&v)
            }
            // Okta: {"op": "replace", "value": {"id": "…", "displayName": "…"}}
            ("replace" | "add", None, Some(Value::Object(values))) => {
                for (k, v) in values {
                    if k.eq_ignore_ascii_case("displayName") {
                        if let Some(name) = as_string(&v) {
                            input.display_name = name;
                        }
                    } else if k.eq_ignore_ascii_case("members") {
                        if kind == "add" {
                            add.extend(member_ids(&v));
                        } else {
                            set = Some(member_ids(&v));
                        }
                    }
                }
            }
            (op, path, _) => {
                return error(
                    StatusCode::BAD_REQUEST,
                    Some("invalidPath"),
                    format!("unsupported group operation {op} on {path:?}"),
                );
            }
        }
    }
    let renamed = (input != group.input).then_some(&input);
    match s.store.update_group(id, renamed, set.as_deref(), &add, &remove).await {
        Ok(Some(g)) => {
            tracing::info!(group = %g.input.display_name, members = g.members.len(), "SCIM: group updated");
            scim(StatusCode::OK, group_json(&g))
        }
        Ok(None) => not_found("group"),
        Err(e) => store_error(e),
    }
}

async fn delete_group(State(s): State<Scim>, Path(id): Path<Uuid>) -> Response {
    match s.store.delete_group(id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => not_found("group"),
        Err(e) => db_error(e),
    }
}

async fn service_provider_config() -> Response {
    scim(
        StatusCode::OK,
        json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig"],
            "patch": { "supported": true },
            "bulk": { "supported": false, "maxOperations": 0, "maxPayloadSize": 0 },
            "filter": { "supported": true, "maxResults": MAX_PAGE },
            "changePassword": { "supported": false },
            "sort": { "supported": false },
            "etag": { "supported": false },
            "authenticationSchemes": [{ "type": "oauthbearertoken", "name": "Bearer token", "description": "The token set in AWZY_SCIM_TOKEN" }],
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_patch_reaches_the_enterprise_extensions_attributes() {
        let mut r = Map::new();
        let department = format!("{ENTERPRISE_SCHEMA}:department");
        set_attribute(&mut r, &department, json!("finance"));
        set_attribute(&mut r, &format!("{ENTERPRISE_SCHEMA}:manager"), json!("mgr-id"));
        assert_eq!(Value::Object(r.clone()), json!({ ENTERPRISE_SCHEMA: { "department": "finance", "manager": "mgr-id" } }));
        set_attribute(&mut r, &department, Value::Null);
        assert_eq!(Value::Object(r.clone()), json!({ ENTERPRISE_SCHEMA: { "manager": "mgr-id" } }));
        set_attribute(&mut r, ENTERPRISE_SCHEMA, json!({ "division": "east" }));
        assert_eq!(Value::Object(r.clone()), json!({ ENTERPRISE_SCHEMA: { "division": "east" } }));
        set_attribute(&mut r, "title", json!("Analyst"));
        assert_eq!(r.get("title"), Some(&json!("Analyst")));
    }

    #[test]
    fn parses_the_filters_idps_send() {
        assert_eq!(
            eq_filter(&Some(r#"userName eq "alice@acme.com""#.into()), "userName").ok(),
            Some(Some("alice@acme.com".into()))
        );
        assert_eq!(eq_filter(&Some(r#"USERNAME EQ "a""#.into()), "userName").ok(), Some(Some("a".into())));
        assert!(eq_filter(&Some(r#"emails co "acme""#.into()), "userName").is_err());
        assert_eq!(eq_filter(&None, "userName").ok(), Some(None));
        let id = Uuid::now_v7();
        assert_eq!(member_filter(&format!(r#"members[value eq "{id}"]"#)), Some(id));
        assert_eq!(as_bool(&json!("False")), Some(false));
        assert_eq!(as_bool(&json!("True")), Some(true));
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            /// Any value an IdP filters on, quotes escaped as SCIM does, comes back as sent,
            /// whatever the attribute's and operator's case.
            #[test]
            fn an_eq_filter_returns_its_value(value in "[^\\\\]*", upper in any::<bool>()) {
                let (attr, op) = if upper { ("USERNAME", "EQ") } else { ("userName", "eq") };
                let filter = format!(r#"{attr} {op} "{}""#, value.replace('"', "\\\""));
                prop_assert_eq!(eq_filter(&Some(filter), "userName").ok(), Some(Some(value)));
            }

            #[test]
            fn a_member_filter_returns_its_id(id in any::<u128>().prop_map(Uuid::from_u128)) {
                prop_assert_eq!(member_filter(&format!(r#"members[value eq "{id}"]"#)), Some(id));
            }

            #[test]
            fn filters_never_panic(s in ".*") {
                let _ = eq_filter(&Some(s.clone()), "userName");
                let _ = member_filter(&s);
            }
        }
    }
}

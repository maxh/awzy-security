//! Jira Cloud: issues as work items, with their assignees and the Google Drive files they cite
//! (ADR 0070), so `context.assigned` (ADR 0046) has real work behind it. Read-only.
//!
//! A full crawl of open issues on start and once a day, which deletes what it no longer sees;
//! between them, the issues updated since the last round, closed ones included.

use std::{
    collections::BTreeSet,
    sync::LazyLock,
    time::{Duration, Instant},
};

use anyhow::{bail, Context};
use awzy_server::metrics::{Counter, LastSuccess, Source};
use axum::http::{Method, StatusCode};
use base64::Engine;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

use crate::{
    context::ContextClient,
    drive,
    http::{encode, Body, Http},
};

pub const SOURCE: &str = "jira";

const FIELDS: [&str; 6] = ["summary", "status", "assignee", "issuetype", "project", "description"];

/// How often the crawl that also finds deleted issues runs.
const FULL_CRAWL_EVERY: Duration = Duration::from_secs(24 * 3600);

/// Waits between retries when Jira says to slow down.
const BACKOFF: [u64; 5] = [1, 2, 4, 8, 16];

pub struct JiraConfig {
    /// https://<site>.atlassian.net, or a fake in tests.
    pub base_url: String,
    /// The connector's Atlassian account, and its API token.
    pub user: String,
    pub token: String,
    /// Project keys in scope; empty means every project the account can browse.
    pub projects: Vec<String>,
    pub interval: Duration,
    /// Counts an issue's watchers as working on it, like its assignee. Costs a call per issue.
    pub watchers: bool,
}

/// The settings the client portal keeps for Jira (ADR 0104).
#[derive(Deserialize)]
struct JiraSettings {
    #[serde(default)]
    url: String,
    #[serde(default)]
    user: String,
    #[serde(default)]
    projects: Vec<String>,
    #[serde(default)]
    watchers: bool,
    interval_secs: Option<u64>,
}

impl JiraConfig {
    /// From the settings the client portal set (ADR 0104), `url`, `user`, and optionally
    /// `projects`, `watchers`, and `interval_secs`, and the API token from the secrets manager.
    pub fn from_settings(s: &serde_json::Value, token: &str) -> anyhow::Result<Self> {
        let s = JiraSettings::deserialize(s).context("Jira's settings")?;
        let text = |v: &str| Some(v.trim().to_string()).filter(|v| !v.is_empty());
        let base_url = text(&s.url).context("Jira's settings have no url")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            user: text(&s.user).context("Jira's settings have no user")?,
            token: token.trim().to_string(),
            projects: s.projects.iter().filter_map(|p| text(p)).collect(),
            interval: Duration::from_secs(s.interval_secs.unwrap_or(300)),
            watchers: s.watchers,
        })
    }

    /// `project in (…) AND ` when scoped, so it prefixes the rest of a query.
    fn scope(&self) -> String {
        if self.projects.is_empty() {
            return String::new();
        }
        let keys: Vec<String> = self.projects.iter().map(|p| format!("\"{}\"", p.replace('"', ""))).collect();
        format!("project in ({}) AND ", keys.join(", "))
    }
}

/// Jira Cloud's REST API v3, as much of it as the connector reads (#221).
pub mod api {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    /// The body of `POST /rest/api/3/search/jql`.
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SearchRequest<'a> {
        pub jql: &'a str,
        pub fields: &'a [&'a str],
        pub max_results: u32,
        /// Left out for the first page: Jira refuses a null.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub next_page_token: Option<String>,
    }

    #[derive(Deserialize, Default)]
    pub struct Issue {
        #[serde(default)]
        pub id: String,
        #[serde(default)]
        pub key: String,
        #[serde(default)]
        pub fields: IssueFields,
    }

    #[derive(Deserialize, Default)]
    pub struct IssueFields {
        pub summary: Option<String>,
        pub status: Option<Status>,
        pub assignee: Option<User>,
        pub issuetype: Option<Named>,
        pub project: Option<Project>,
        /// Atlassian Document Format, walked for links rather than read field by field.
        #[serde(default)]
        pub description: Value,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Status {
        pub status_category: Option<StatusCategory>,
    }

    #[derive(Deserialize)]
    pub struct StatusCategory {
        pub key: String,
    }

    /// Someone on an issue. Jira leaves `emailAddress` out when their profile hides it.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct User {
        pub email_address: Option<String>,
    }

    #[derive(Deserialize)]
    pub struct Named {
        pub name: Option<String>,
    }

    #[derive(Deserialize)]
    pub struct Project {
        pub key: Option<String>,
    }

    /// A page of `POST /rest/api/3/search/jql`.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SearchPage {
        #[serde(default)]
        pub issues: Vec<Issue>,
        pub next_page_token: Option<String>,
        pub is_last: Option<bool>,
    }

    /// One of `GET /rest/api/3/issue/{id}/remotelink`.
    #[derive(Deserialize)]
    pub struct RemoteLink {
        pub object: Option<RemoteObject>,
    }

    #[derive(Deserialize)]
    pub struct RemoteObject {
        pub url: Option<String>,
    }

    /// `GET /rest/api/3/issue/{id}/watchers`.
    #[derive(Deserialize)]
    pub struct Watchers {
        #[serde(default)]
        pub watchers: Vec<User>,
    }
}

/// The Drive file or folder ID in a Google Drive or Docs URL, if it is one.
pub fn drive_file_id(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    if !matches!(host, "docs.google.com" | "drive.google.com") {
        return None;
    }
    // `open?id=…`: the ID is a query parameter, not part of another one (`#gid=0`).
    let param = path.split(['?', '&']).skip(1).find_map(|p| p.strip_prefix("id="));
    let id = if let Some(id) = param {
        Some(id.split(['&', '#']).next().unwrap_or(id))
    } else {
        let parts: Vec<&str> = path.split(['/', '?', '#']).collect();
        parts.iter().position(|p| *p == "d" || *p == "folders").and_then(|i| parts.get(i + 1)).copied()
    }?;
    let valid = id.len() >= 10 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    valid.then(|| id.to_string())
}

/// Every URL in a Jira description (Atlassian Document Format): link marks, cards, and URLs
/// written as plain text.
pub fn description_urls(adf: &Value) -> Vec<String> {
    let mut urls = Vec::new();
    fn walk(v: &Value, urls: &mut Vec<String>) {
        match v {
            Value::Object(map) => {
                for (key, value) in map {
                    match (key.as_str(), value) {
                        ("href" | "url", Value::String(u)) => urls.push(u.clone()),
                        ("text", Value::String(t)) => urls.extend(
                            t.split_whitespace()
                                .filter(|w| w.starts_with("https://"))
                                .map(|w| w.trim_end_matches(|c: char| ".,;:)]>".contains(c)).to_string()),
                        ),
                        _ => walk(value, urls),
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|i| walk(i, urls)),
            _ => {}
        }
    }
    walk(adf, &mut urls);
    urls
}

/// An issue as a work item. `links` is every URL it cites: its description's and its remote links'.
/// `watchers`: the issue's watchers, counted as working on it beside its assignee (an empty slice
/// when that setting is off). Anyone whose email Jira hides is counted, not guessed.
pub fn work_item(issue: &api::Issue, links: &[String], watchers: &[api::User]) -> Value {
    let fields = &issue.fields;
    let key = issue.key.as_str();
    let done = fields.status.as_ref().and_then(|s| s.status_category.as_ref()).is_some_and(|c| c.key == "done");
    let mut emails = BTreeSet::new();
    let mut unmapped = 0u64;
    for human in fields.assignee.iter().chain(watchers) {
        match human.email_address.as_deref().filter(|e| !e.is_empty()) {
            Some(email) => {
                emails.insert(email.to_lowercase());
            }
            None => unmapped += 1,
        }
    }
    let resources: BTreeSet<String> =
        links.iter().filter_map(|u| drive_file_id(u)).map(|id| drive::resource_id(&id)).collect();
    let other_links = links.iter().filter(|u| drive_file_id(u).is_none()).count();
    json!({
        "id": format!("jira:{}", issue.id),
        "source": SOURCE,
        "kind": "ticket",
        "title": format!("{key}: {}", fields.summary.as_deref().unwrap_or_default()),
        "state": if done { "closed" } else { "open" },
        "assignees": emails,
        "resources": resources,
        "attributes": {
            "key": key,
            "issue_type": fields.issuetype.as_ref().and_then(|t| t.name.as_deref()),
            "project": fields.project.as_ref().and_then(|p| p.key.as_deref()),
            "unmapped_assignees": unmapped,
            "other_links": other_links,
        },
    })
}

/// When the last round succeeded, and what it wrote (ADR 0068).
pub static SYNC: LazyLock<LastSuccess> = LazyLock::new(LastSuccess::default);
pub static WRITTEN: Counter = Counter::new();
pub static DELETED: Counter = Counter::new();

pub struct JiraMetrics;

impl Source for JiraMetrics {
    fn render(&self, out: &mut String) {
        SYNC.render(out, "awzy_connectors_jira_sync", "a Jira sync round");
        out.push_str(
            "# HELP awzy_connectors_jira_work_items_total Jira work items written to or deleted from awzy-context.\n",
        );
        out.push_str("# TYPE awzy_connectors_jira_work_items_total counter\n");
        out.push_str(&format!("awzy_connectors_jira_work_items_total{{change=\"written\"}} {}\n", WRITTEN.get()));
        out.push_str(&format!("awzy_connectors_jira_work_items_total{{change=\"deleted\"}} {}\n", DELETED.get()));
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub written: usize,
    pub deleted: usize,
}

pub struct JiraSync {
    cfg: JiraConfig,
    http: Http,
    context: ContextClient,
    last_full: Option<Instant>,
    /// When the last successful round started: the next one asks for what changed since.
    last_round: Option<Instant>,
}

impl JiraSync {
    pub fn new(cfg: JiraConfig, http: Http, context: ContextClient) -> Self {
        Self { cfg, http, context, last_full: None, last_round: None }
    }

    pub async fn run(mut self) {
        loop {
            let full = self.last_full.is_none_or(|t| t.elapsed() >= FULL_CRAWL_EVERY);
            let round = if full { self.full_crawl().await } else { self.changed().await };
            match round {
                Ok(s) => {
                    SYNC.succeeded();
                    crate::status::succeeded("jira", s.written, s.deleted);
                    tracing::info!(full, written = s.written, deleted = s.deleted, "Jira synced");
                }
                Err(error) => {
                    SYNC.failed();
                    crate::status::failed("jira", &error);
                    tracing::warn!(error = format!("{error:#}"), "Jira sync failed; retrying next round");
                }
            }
            tokio::time::sleep(self.cfg.interval).await;
        }
    }

    async fn call<T: DeserializeOwned>(&self, method: Method, path: &str, body: Option<&Value>) -> anyhow::Result<T> {
        let url = format!("{}{path}", self.cfg.base_url);
        let basic = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", self.cfg.user, self.cfg.token));
        let headers = [("authorization", format!("Basic {basic}")), ("accept", "application/json".to_string())];
        for wait in BACKOFF.iter().map(|s| Some(Duration::from_secs(*s))).chain([None]) {
            let body = body.map_or(Body::None, Body::Json);
            let (status, res) = self.http.send(method.clone(), &url, &headers, body).await?;
            if status.is_success() {
                let what = path.split('?').next().unwrap_or(path);
                return T::deserialize(res).with_context(|| format!("Jira's answer for {what}"));
            }
            match wait {
                Some(wait) if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() => {
                    tokio::time::sleep(wait).await;
                }
                _ => bail!("Jira answered {status} for {}: {res}", path.split('?').next().unwrap_or(path)),
            }
        }
        unreachable!("the last attempt either returns or fails")
    }

    /// Every issue matching `jql`, page by page.
    async fn search(&self, jql: &str) -> anyhow::Result<Vec<api::Issue>> {
        let mut issues = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let req = api::SearchRequest { jql, fields: &FIELDS, max_results: 100, next_page_token: page.take() };
            let body = serde_json::to_value(req)?;
            let res: api::SearchPage = self.call(Method::POST, "/rest/api/3/search/jql", Some(&body)).await?;
            issues.extend(res.issues);
            match res.next_page_token {
                Some(next) if res.is_last != Some(true) => page = Some(next),
                _ => return Ok(issues),
            }
        }
    }

    /// The issue as a work item, with the links in its description and its remote links.
    async fn item(&self, issue: &api::Issue) -> anyhow::Result<Value> {
        let mut links = description_urls(&issue.fields.description);
        if issue.id.is_empty() {
            bail!("an issue without an id");
        }
        let id = encode(&issue.id);
        let remote: Vec<api::RemoteLink> =
            self.call(Method::GET, &format!("/rest/api/3/issue/{id}/remotelink"), None).await?;
        links.extend(remote.into_iter().filter_map(|l| l.object?.url));
        let watchers = if self.cfg.watchers {
            let res: api::Watchers = self.call(Method::GET, &format!("/rest/api/3/issue/{id}/watchers"), None).await?;
            res.watchers
        } else {
            vec![]
        };
        Ok(work_item(issue, &links, &watchers))
    }

    async fn write(&self, issues: &[api::Issue]) -> anyhow::Result<usize> {
        let mut items = Vec::with_capacity(issues.len());
        for issue in issues {
            items.push(self.item(issue).await?);
        }
        self.context.put_work_items(&items).await?;
        WRITTEN.inc_by(items.len() as u64);
        Ok(items.len())
    }

    /// Every open issue in scope; then every `jira` work item it didn't see is deleted.
    pub async fn full_crawl(&mut self) -> anyhow::Result<Summary> {
        let started = Instant::now();
        let issues = self.search(&format!("{}statusCategory != Done ORDER BY id", self.cfg.scope())).await?;
        let mut summary = Summary { written: self.write(&issues).await?, deleted: 0 };
        let seen: BTreeSet<String> =
            issues.iter().filter(|i| !i.id.is_empty()).map(|i| format!("jira:{}", i.id)).collect();
        for gone in self.context.work_item_ids(SOURCE).await?.difference(&seen) {
            self.context.delete_work_item(gone).await?;
            DELETED.inc();
            summary.deleted += 1;
        }
        self.last_full = Some(started);
        self.last_round = Some(started);
        Ok(summary)
    }

    /// The issues updated since the last round began, closed ones included. A minute's margin
    /// covers clock skew and Jira's index lag.
    pub async fn changed(&mut self) -> anyhow::Result<Summary> {
        let started = Instant::now();
        let since = self.last_round.map_or(FULL_CRAWL_EVERY, |t| t.elapsed());
        let minutes = since.as_secs().div_ceil(60) + 1;
        let issues = self.search(&format!("{}updated >= -{minutes}m ORDER BY id", self.cfg.scope())).await?;
        let summary = Summary { written: self.write(&issues).await?, deleted: 0 };
        self.last_round = Some(started);
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use axum::{
        extract::{Path, Query, State},
        routing::{delete, get, post},
        Json, Router,
    };

    use super::*;

    #[test]
    fn finds_drive_files_in_any_google_url_shape() {
        let id = "1AbCdEfGhIjKlMnOp_qr-st";
        for url in [
            format!("https://docs.google.com/spreadsheets/d/{id}/edit#gid=0"),
            format!("https://docs.google.com/document/d/{id}"),
            format!("https://drive.google.com/file/d/{id}/view?usp=sharing"),
            format!("https://drive.google.com/drive/folders/{id}"),
            format!("https://drive.google.com/open?id={id}&authuser=0"),
        ] {
            assert_eq!(drive_file_id(&url).as_deref(), Some(id), "{url}");
        }
        assert_eq!(drive_file_id("https://example.com/d/1AbCdEfGhIjKlMnOp"), None);
        assert_eq!(drive_file_id("https://docs.google.com/document/u/0/"), None);
    }

    #[test]
    fn reads_links_from_marks_cards_and_plain_text() {
        let adf = json!({ "type": "doc", "content": [
            { "type": "paragraph", "content": [
                { "type": "text", "text": "Plan", "marks": [{ "type": "link", "attrs": { "href": "https://a.example/x" } }] },
                { "type": "text", "text": " and see https://b.example/y." },
                { "type": "inlineCard", "attrs": { "url": "https://c.example/z" } },
            ] },
        ] });
        assert_eq!(description_urls(&adf), ["https://a.example/x", "https://b.example/y", "https://c.example/z"]);
    }

    #[test]
    fn watchers_count_as_working_on_it_once_each() {
        let issue = json!({ "id": "10002", "key": "ENG-3", "fields": {
            "summary": "Rollout", "status": { "statusCategory": { "key": "indeterminate" } },
            "assignee": { "accountId": "a", "emailAddress": "Priya@larkspur.example" },
            "issuetype": { "name": "Task" }, "project": { "key": "ENG" } } });
        let issue: api::Issue = serde_json::from_value(issue).unwrap();
        let watchers: Vec<api::User> = serde_json::from_value(json!([
            { "accountId": "a", "emailAddress": "priya@larkspur.example" },
            { "accountId": "b", "emailAddress": "sam@larkspur.example" },
            { "accountId": "c" },
        ]))
        .unwrap();
        let w = work_item(&issue, &[], &watchers);
        assert_eq!(w["assignees"], json!(["priya@larkspur.example", "sam@larkspur.example"]));
        assert_eq!(w["attributes"]["unmapped_assignees"].as_u64(), Some(1), "the hidden email is counted");
        assert_eq!(work_item(&issue, &[], &[])["assignees"], json!(["priya@larkspur.example"]));
    }

    #[test]
    fn a_done_issue_is_closed_and_a_hidden_email_is_counted_not_guessed() {
        let issue = json!({ "id": "10001", "key": "FIN-7", "fields": {
            "summary": "Q3 audit", "status": { "statusCategory": { "key": "done" } },
            "assignee": { "accountId": "abc" }, "issuetype": { "name": "Task" }, "project": { "key": "FIN" } } });
        let issue: api::Issue = serde_json::from_value(issue).unwrap();
        let w = work_item(
            &issue,
            &["https://docs.google.com/spreadsheets/d/1AbCdEfGhIjKlMnOp/edit".into(), "https://x.example".into()],
            &[],
        );
        assert_eq!(
            (w["id"].as_str(), w["state"].as_str(), w["title"].as_str()),
            (Some("jira:10001"), Some("closed"), Some("FIN-7: Q3 audit"))
        );
        assert_eq!(w["assignees"], json!([]));
        assert_eq!(w["resources"], json!(["gdrive:1AbCdEfGhIjKlMnOp"]));
        assert_eq!(
            (w["attributes"]["unmapped_assignees"].as_u64(), w["attributes"]["other_links"].as_u64()),
            (Some(1), Some(1))
        );
    }

    #[derive(Default)]
    struct Fake {
        issues: Vec<Value>,
        queries: Vec<String>,
        written: Vec<Value>,
        deleted: Vec<String>,
        existing: Vec<String>,
    }

    type Shared = Arc<Mutex<Fake>>;

    async fn serve(fake: Shared) -> String {
        let app = Router::new()
            .route(
                "/rest/api/3/search/jql",
                post(|State(f): State<Shared>, Json(b): Json<Value>| async move {
                    let mut f = f.lock().unwrap();
                    f.queries.push(b["jql"].as_str().unwrap().to_string());
                    Json(json!({ "issues": f.issues, "isLast": true }))
                }),
            )
            .route(
                "/rest/api/3/issue/{id}/remotelink",
                get(|Path(id): Path<String>| async move {
                    let links = if id == "1" {
                        json!([{ "object": { "url": "https://drive.google.com/file/d/1RemoteLinkedFile/view" } }])
                    } else {
                        json!([])
                    };
                    Json(links)
                }),
            )
            .route(
                "/v1/work-items/batch",
                post(|State(f): State<Shared>, Json(b): Json<Value>| async move {
                    f.lock().unwrap().written.extend(b["work_items"].as_array().unwrap().clone());
                    Json(json!({}))
                }),
            )
            .route(
                "/v1/work-items/{id}",
                delete(|State(f): State<Shared>, Path(id): Path<String>| async move {
                    f.lock().unwrap().deleted.push(id);
                    StatusCode::NO_CONTENT
                }),
            )
            .route(
                "/v1/work-items",
                get(|State(f): State<Shared>, Query(q): Query<HashMap<String, String>>| async move {
                    let f = f.lock().unwrap();
                    assert_eq!(q["source"], "jira");
                    let page: Vec<Value> = if q.contains_key("after") {
                        vec![]
                    } else {
                        f.existing.iter().map(|id| json!({ "id": id })).collect()
                    };
                    Json(json!({ "work_items": page }))
                }),
            )
            .with_state(fake);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn issue(id: &str, key: &str, status: &str, email: &str, description: Value) -> Value {
        json!({ "id": id, "key": key, "fields": {
            "summary": "Close the books", "status": { "statusCategory": { "key": status } },
            "assignee": { "accountId": "a", "emailAddress": email }, "issuetype": { "name": "Task" },
            "project": { "key": "FIN" }, "description": description } })
    }

    #[tokio::test]
    async fn a_crawl_writes_open_issues_with_their_drive_files_then_changes_close_them() {
        let fake: Shared = Arc::default();
        let description = json!({ "type": "doc", "content": [{ "type": "inlineCard", "attrs": { "url": "https://docs.google.com/spreadsheets/d/1DescribedSheet/edit" } }] });
        {
            let mut f = fake.lock().unwrap();
            f.issues = vec![issue("1", "FIN-1", "indeterminate", "Priya@Larkspur.example", description)];
            f.existing = vec!["jira:1".into(), "jira:99".into()];
        }
        let base = serve(fake.clone()).await;
        let http = Http::default();
        let context = ContextClient::new(&base, http.clone(), awzy_service_auth::TokenSource::off());
        let cfg = JiraConfig {
            base_url: base.clone(),
            user: "awzy@larkspur.example".into(),
            token: "t".into(),
            projects: vec!["FIN".into()],
            interval: Duration::from_secs(300),
            watchers: false,
        };
        let mut sync = JiraSync::new(cfg, http, context);

        assert_eq!(sync.full_crawl().await.unwrap(), Summary { written: 1, deleted: 1 });
        {
            let f = fake.lock().unwrap();
            assert_eq!(f.queries[0], "project in (\"FIN\") AND statusCategory != Done ORDER BY id");
            let w = &f.written[0];
            assert_eq!(w["assignees"], json!(["priya@larkspur.example"]));
            assert_eq!(w["resources"], json!(["gdrive:1DescribedSheet", "gdrive:1RemoteLinkedFile"]));
            assert_eq!(f.deleted, ["jira:99"], "deleted or closed while the connector was down");
        }

        fake.lock().unwrap().issues = vec![issue("1", "FIN-1", "done", "priya@larkspur.example", Value::Null)];
        assert_eq!(sync.changed().await.unwrap(), Summary { written: 1, deleted: 0 });
        let f = fake.lock().unwrap();
        assert_eq!(
            f.queries[1], "project in (\"FIN\") AND updated >= -1m ORDER BY id",
            "since the crawl, plus a minute"
        );
        assert_eq!(f.written[1]["state"], "closed", "stops counting within one round");
    }
}

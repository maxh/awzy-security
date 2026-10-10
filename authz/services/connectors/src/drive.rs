//! Google Drive: shared drives' files and folders, with their sharing, as awzy-context resources
//! (ADR 0069). Metadata only, read-only.
//!
//! A full crawl on start, then each drive's change feed. Files in shared drives inherit the
//! drive's members through `parent`; a file's own permissions are read only when Drive flags them
//! (`hasAugmentedPermissions`), since listing every file's would cost a request per file.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, LazyLock},
    time::Duration,
};

use anyhow::{bail, Context};
use awzy_server::metrics::{Counter, LastSuccess, Source};
use axum::http::{Method, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    context::ContextClient,
    google::{Auth, DIRECTORY_GROUPS, DRIVE_METADATA},
    http::{encode, Body, Http},
};

pub const SOURCE: &str = "google_drive";

const FILE_FIELDS: &str = "id,name,mimeType,parents,webViewLink,modifiedTime,trashed,hasAugmentedPermissions,driveId";
const PERMISSION_FIELDS: &str = "type,role,emailAddress,domain,permissionDetails(inherited)";

/// Waits between retries when Google says to slow down.
const BACKOFF: [u64; 5] = [1, 2, 4, 8, 16];

pub struct DriveConfig {
    /// The member user the connector acts as.
    pub subject: String,
    /// An admin, to read the domain's groups; without one, group shares keep their emails.
    pub admin_subject: Option<String>,
    /// The client's own domain: only its `domain` shares mean everyone.
    pub domain: String,
    /// https://www.googleapis.com, or a fake in tests.
    pub api_base: String,
    pub interval: Duration,
}

impl DriveConfig {
    /// From `AWZY_CONNECTORS_DRIVE_*`. None when `AWZY_CONNECTORS_DRIVE_SUBJECT` isn't set: Drive
    /// is off.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let Ok(subject) = std::env::var("AWZY_CONNECTORS_DRIVE_SUBJECT") else { return Ok(None) };
        let domain = std::env::var("AWZY_CONNECTORS_DRIVE_DOMAIN")
            .ok()
            .or_else(|| subject.split_once('@').map(|(_, d)| d.to_string()))
            .context("AWZY_CONNECTORS_DRIVE_DOMAIN is needed when the subject isn't an email")?;
        let interval =
            std::env::var("AWZY_CONNECTORS_DRIVE_INTERVAL_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
        Ok(Some(Self {
            subject,
            admin_subject: std::env::var("AWZY_CONNECTORS_DRIVE_ADMIN_SUBJECT").ok().filter(|s| !s.is_empty()),
            domain: domain.to_lowercase(),
            api_base: "https://www.googleapis.com".into(),
            interval: Duration::from_secs(interval),
        }))
    }
}

/// How the connector gets Google access tokens.
pub enum Credentials {
    ServiceAccount(Box<Auth>),
    /// One token for everything. For tests against a fake Google.
    Fixed(String),
}

impl Credentials {
    async fn token(&self, subject: &str, scope: &str) -> anyhow::Result<String> {
        match self {
            Self::ServiceAccount(auth) => auth.token(subject, scope).await,
            Self::Fixed(token) => Ok(token.clone()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SharedDrive {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct File {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub parents: Vec<String>,
    pub web_view_link: Option<String>,
    pub modified_time: Option<String>,
    pub trashed: bool,
    pub has_augmented_permissions: bool,
    pub drive_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Permission {
    #[serde(rename = "type")]
    pub kind: String,
    pub role: String,
    pub email_address: Option<String>,
    pub domain: Option<String>,
    pub permission_details: Vec<PermissionDetail>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PermissionDetail {
    pub inherited: bool,
}

impl Permission {
    /// Set on this file itself, not inherited from its folders or drive.
    fn is_own(&self) -> bool {
        self.permission_details.is_empty() || self.permission_details.iter().any(|d| !d.inherited)
    }
}

/// awzy-context's ID for a Drive file, folder, or shared drive.
pub fn resource_id(drive_id: &str) -> String {
    format!("gdrive:{drive_id}")
}

/// A short kind from Drive's MIME type.
pub fn kind(mime_type: &str) -> &'static str {
    match mime_type {
        "application/vnd.google-apps.folder" => "folder",
        "application/vnd.google-apps.document" => "document",
        "application/vnd.google-apps.spreadsheet" => "spreadsheet",
        "application/vnd.google-apps.presentation" => "presentation",
        "application/vnd.google-apps.form" => "form",
        "application/pdf" => "pdf",
        _ => "file",
    }
}

/// Drive's role as awzy-context's: organizers and file organizers can change what they hold.
fn role(drive_role: &str) -> &'static str {
    match drive_role {
        "owner" => "owner",
        "organizer" | "fileOrganizer" | "writer" => "writer",
        "commenter" => "commenter",
        _ => "reader",
    }
}

/// Drive permissions as awzy-context shares, and how many reach outside `domain` (ADR 0069).
/// `groups` maps a group's email to its name, as SCIM provisions it.
pub fn sharing(perms: &[Permission], domain: &str, groups: &HashMap<String, String>) -> (Vec<Value>, u64) {
    let in_domain = |email: &str| email.rsplit_once('@').is_some_and(|(_, d)| d.eq_ignore_ascii_case(domain));
    let mut shares = Vec::new();
    let mut external = 0;
    for p in perms {
        let role = role(&p.role);
        let email = p.email_address.as_deref().map(str::to_lowercase);
        match (p.kind.as_str(), email) {
            ("user", Some(email)) if in_domain(&email) => {
                shares.push(json!({ "kind": "person", "id": email, "role": role }));
            }
            ("group", Some(email)) if in_domain(&email) => {
                let id = groups.get(&email).cloned().unwrap_or(email);
                shares.push(json!({ "kind": "group", "id": id, "role": role }));
            }
            ("domain", _) if p.domain.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(domain)) => {
                shares.push(json!({ "kind": "domain", "id": domain, "role": role }));
            }
            ("anyone", _) => {
                shares.push(json!({ "kind": "anyone", "id": "", "role": role }));
                external += 1;
            }
            _ => external += 1,
        }
    }
    (shares, external)
}

/// A shared drive as a resource, its members as sharing.
pub fn drive_resource(
    drive: &SharedDrive,
    members: &[Permission],
    cfg: &DriveConfig,
    groups: &HashMap<String, String>,
) -> Value {
    let (sharing, external) = sharing(members, &cfg.domain, groups);
    json!({
        "id": resource_id(&drive.id),
        "source": SOURCE,
        "kind": "shared_drive",
        "name": drive.name,
        "sharing": sharing,
        "url": format!("https://drive.google.com/drive/folders/{}", drive.id),
        "attributes": { "drive_id": drive.id, "external_shares": external },
    })
}

/// A file or folder as a resource. `own` is its own permissions, empty unless Drive flagged some.
pub fn file_resource(file: &File, own: &[Permission], cfg: &DriveConfig, groups: &HashMap<String, String>) -> Value {
    let own: Vec<Permission> = own.iter().filter(|p| p.is_own()).cloned().collect();
    let (sharing, external) = sharing(&own, &cfg.domain, groups);
    // At the top of a shared drive, the parent is the drive's root folder, which has the drive's ID.
    let parent = file.parents.first().or(file.drive_id.as_ref()).map(|p| resource_id(p));
    json!({
        "id": resource_id(&file.id),
        "source": SOURCE,
        "kind": kind(&file.mime_type),
        "name": file.name,
        "parent": parent,
        "sharing": sharing,
        "url": file.web_view_link,
        "attributes": {
            "mime_type": file.mime_type,
            "modified_time": file.modified_time,
            "drive_id": file.drive_id,
            "external_shares": external,
        },
    })
}

/// When the last sync round succeeded, and what it wrote (ADR 0068).
pub static SYNC: LazyLock<LastSuccess> = LazyLock::new(LastSuccess::default);
pub static WRITTEN: Counter = Counter::new();
pub static DELETED: Counter = Counter::new();

pub struct DriveMetrics;

impl Source for DriveMetrics {
    fn render(&self, out: &mut String) {
        SYNC.render(out, "awzy_connectors_drive_sync", "a Google Drive sync round");
        out.push_str(
            "# HELP awzy_connectors_drive_resources_total Drive resources written to or deleted from awzy-context.\n",
        );
        out.push_str("# TYPE awzy_connectors_drive_resources_total counter\n");
        out.push_str(&format!("awzy_connectors_drive_resources_total{{change=\"written\"}} {}\n", WRITTEN.get()));
        out.push_str(&format!("awzy_connectors_drive_resources_total{{change=\"deleted\"}} {}\n", DELETED.get()));
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub written: usize,
    pub deleted: usize,
}

pub struct DriveSync {
    cfg: DriveConfig,
    credentials: Arc<Credentials>,
    http: Http,
    context: ContextClient,
    /// Each drive's change cursor, from the last round. Empty until a full crawl.
    cursors: BTreeMap<String, String>,
    groups: HashMap<String, String>,
}

impl DriveSync {
    pub fn new(cfg: DriveConfig, credentials: Arc<Credentials>, http: Http, context: ContextClient) -> Self {
        Self { cfg, credentials, http, context, cursors: BTreeMap::new(), groups: HashMap::new() }
    }

    /// A full crawl, then the change feed every interval, forever. A failed round is retried
    /// at the next; a failed crawl is retried before any change round.
    pub async fn run(mut self) {
        loop {
            let round = if self.cursors.is_empty() { self.full_crawl().await } else { self.poll_changes().await };
            match round {
                Ok(s) => {
                    SYNC.succeeded();
                    tracing::info!(written = s.written, deleted = s.deleted, "Google Drive synced");
                }
                Err(error) => {
                    SYNC.failed();
                    tracing::warn!(error = format!("{error:#}"), "Google Drive sync failed; retrying next round");
                }
            }
            tokio::time::sleep(self.cfg.interval).await;
        }
    }

    async fn get(&self, subject: &str, scope: &str, path: &str) -> anyhow::Result<Value> {
        let url = format!("{}{path}", self.cfg.api_base);
        for wait in BACKOFF.iter().map(|s| Some(Duration::from_secs(*s))).chain([None]) {
            let token = self.credentials.token(subject, scope).await?;
            let headers = [("authorization", format!("Bearer {token}"))];
            let (status, body) = self.http.send(Method::GET, &url, &headers, Body::None).await?;
            if status.is_success() {
                return Ok(body);
            }
            let slow_down = status == StatusCode::TOO_MANY_REQUESTS
                || (status == StatusCode::FORBIDDEN && body.to_string().contains("RateLimitExceeded"))
                || status.is_server_error();
            match wait {
                Some(wait) if slow_down => tokio::time::sleep(wait).await,
                _ => bail!("Google answered {status} for {}: {body}", path.split('?').next().unwrap_or(path)),
            }
        }
        unreachable!("the last attempt either returns or fails")
    }

    /// Every page of a Drive list call, collecting `field`.
    async fn pages<T: for<'de> Deserialize<'de>>(&self, path: &str, field: &str) -> anyhow::Result<Vec<T>> {
        let mut items = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let mut p = path.to_string();
            if let Some(token) = &page {
                p.push_str(&format!("&pageToken={}", encode(token)));
            }
            let mut body = self.get(&self.cfg.subject, DRIVE_METADATA, &p).await?;
            let list = body[field].take();
            items.extend(serde_json::from_value::<Vec<T>>(if list.is_null() { json!([]) } else { list })?);
            match body["nextPageToken"].as_str() {
                Some(next) => page = Some(next.to_string()),
                None => return Ok(items),
            }
        }
    }

    async fn drives(&self) -> anyhow::Result<Vec<SharedDrive>> {
        self.pages("/drive/v3/drives?pageSize=100&fields=nextPageToken,drives(id,name)", "drives").await
    }

    async fn permissions(&self, id: &str) -> anyhow::Result<Vec<Permission>> {
        let path = format!(
            "/drive/v3/files/{}/permissions?supportsAllDrives=true&pageSize=100&fields=nextPageToken,permissions({PERMISSION_FIELDS})",
            encode(id)
        );
        self.pages(&path, "permissions").await
    }

    async fn files(&self, drive_id: &str) -> anyhow::Result<Vec<File>> {
        let path = format!(
            "/drive/v3/files?corpora=drive&driveId={}&includeItemsFromAllDrives=true&supportsAllDrives=true&q={}&pageSize=1000&fields=nextPageToken,files({FILE_FIELDS})",
            encode(drive_id),
            encode("trashed = false")
        );
        self.pages(&path, "files").await
    }

    /// The domain's groups, email to name, read as the admin. Empty without one.
    async fn load_groups(&mut self) -> anyhow::Result<()> {
        let Some(admin) = self.cfg.admin_subject.clone() else { return Ok(()) };
        let mut groups = HashMap::new();
        let mut page: Option<String> = None;
        loop {
            let mut path = "/admin/directory/v1/groups?customer=my_customer&maxResults=200".to_string();
            if let Some(token) = &page {
                path.push_str(&format!("&pageToken={}", encode(token)));
            }
            let body = self.get(&admin, DIRECTORY_GROUPS, &path).await?;
            for g in body["groups"].as_array().into_iter().flatten() {
                if let (Some(email), Some(name)) = (g["email"].as_str(), g["name"].as_str()) {
                    groups.insert(email.to_lowercase(), name.to_string());
                }
            }
            match body["nextPageToken"].as_str() {
                Some(next) => page = Some(next.to_string()),
                None => break,
            }
        }
        self.groups = groups;
        Ok(())
    }

    async fn start_cursor(&self, drive_id: &str) -> anyhow::Result<String> {
        let path = format!("/drive/v3/changes/startPageToken?driveId={}&supportsAllDrives=true", encode(drive_id));
        let body = self.get(&self.cfg.subject, DRIVE_METADATA, &path).await?;
        Ok(body["startPageToken"].as_str().context("no startPageToken from Drive")?.to_string())
    }

    async fn file(&self, file: &File) -> anyhow::Result<Value> {
        let own = if file.has_augmented_permissions { self.permissions(&file.id).await? } else { vec![] };
        Ok(file_resource(file, &own, &self.cfg, &self.groups))
    }

    async fn write(&self, resources: &[Value]) -> anyhow::Result<usize> {
        self.context.put_resources(resources).await?;
        WRITTEN.inc_by(resources.len() as u64);
        Ok(resources.len())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        self.context.delete_resource(id).await?;
        DELETED.inc();
        Ok(())
    }

    /// Every shared drive, then everything from this source that the crawl didn't see is deleted.
    /// Each drive's cursor is taken before its files are listed, so nothing changed during the
    /// crawl is missed.
    pub async fn full_crawl(&mut self) -> anyhow::Result<Summary> {
        self.load_groups().await?;
        let mut summary = Summary::default();
        let mut seen = BTreeSet::new();
        let mut cursors = BTreeMap::new();
        for drive in self.drives().await? {
            cursors.insert(drive.id.clone(), self.start_cursor(&drive.id).await?);
            let members = self.permissions(&drive.id).await?;
            let mut resources = vec![drive_resource(&drive, &members, &self.cfg, &self.groups)];
            for file in self.files(&drive.id).await? {
                resources.push(self.file(&file).await?);
            }
            seen.extend(resources.iter().filter_map(|r| r["id"].as_str().map(str::to_string)));
            summary.written += self.write(&resources).await?;
        }
        for gone in self.context.resource_ids(SOURCE).await?.difference(&seen) {
            self.delete(gone).await?;
            summary.deleted += 1;
        }
        self.cursors = cursors;
        Ok(summary)
    }

    /// Each drive's changes since the last round, and its members again. A drive added or
    /// removed since means a full crawl.
    pub async fn poll_changes(&mut self) -> anyhow::Result<Summary> {
        let drives = self.drives().await?;
        if drives.iter().map(|d| &d.id).collect::<BTreeSet<_>>() != self.cursors.keys().collect() {
            return self.full_crawl().await;
        }
        self.load_groups().await?;
        let mut summary = Summary::default();
        for drive in drives {
            let members = self.permissions(&drive.id).await?;
            let mut resources = vec![drive_resource(&drive, &members, &self.cfg, &self.groups)];
            let mut cursor = self.cursors[&drive.id].clone();
            loop {
                let path = format!(
                    "/drive/v3/changes?driveId={}&pageToken={}&includeItemsFromAllDrives=true&supportsAllDrives=true&pageSize=1000&fields=nextPageToken,newStartPageToken,changes(fileId,removed,file({FILE_FIELDS}))",
                    encode(&drive.id),
                    encode(&cursor)
                );
                let body = self.get(&self.cfg.subject, DRIVE_METADATA, &path).await?;
                for change in body["changes"].as_array().into_iter().flatten() {
                    let file: Option<File> = serde_json::from_value(change["file"].clone()).ok();
                    match file {
                        Some(file) if !change["removed"].as_bool().unwrap_or(false) && !file.trashed => {
                            resources.push(self.file(&file).await?);
                        }
                        _ => {
                            let id = change["fileId"].as_str().context("a change without a fileId")?;
                            self.delete(&resource_id(id)).await?;
                            summary.deleted += 1;
                        }
                    }
                }
                if let Some(next) = body["nextPageToken"].as_str() {
                    cursor = next.to_string();
                    continue;
                }
                cursor = body["newStartPageToken"].as_str().context("no newStartPageToken from Drive")?.to_string();
                break;
            }
            summary.written += self.write(&resources).await?;
            self.cursors.insert(drive.id, cursor);
        }
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::{
        extract::{Path, Query, State},
        routing::{delete, get, post},
        Json, Router,
    };

    use super::*;

    fn cfg(api_base: &str) -> DriveConfig {
        DriveConfig {
            subject: "awzy-reader@larkspur.example".into(),
            admin_subject: Some("admin@larkspur.example".into()),
            domain: "larkspur.example".into(),
            api_base: api_base.into(),
            interval: Duration::from_secs(300),
        }
    }

    fn perm(kind: &str, role: &str, email: Option<&str>, domain: Option<&str>) -> Permission {
        Permission {
            kind: kind.into(),
            role: role.into(),
            email_address: email.map(str::to_string),
            domain: domain.map(str::to_string),
            permission_details: vec![],
        }
    }

    #[test]
    fn only_the_clients_domain_means_everyone() {
        let groups = HashMap::from([("finance@larkspur.example".to_string(), "finance".to_string())]);
        let perms = [
            perm("user", "writer", Some("Priya@Larkspur.example"), None),
            perm("group", "reader", Some("finance@larkspur.example"), None),
            perm("domain", "reader", None, Some("larkspur.example")),
            perm("domain", "reader", None, Some("partner.example")),
            perm("user", "reader", Some("auditor@partner.example"), None),
            perm("anyone", "reader", None, None),
        ];
        let (shares, external) = sharing(&perms, "larkspur.example", &groups);
        assert_eq!(
            shares,
            vec![
                json!({ "kind": "person", "id": "priya@larkspur.example", "role": "writer" }),
                json!({ "kind": "group", "id": "finance", "role": "reader" }),
                json!({ "kind": "domain", "id": "larkspur.example", "role": "reader" }),
                json!({ "kind": "anyone", "id": "", "role": "reader" }),
            ]
        );
        assert_eq!(external, 3, "the partner domain, the partner's auditor, and anyone with the link");
    }

    #[test]
    fn organizers_write_and_unknown_groups_keep_their_email() {
        let perms = [
            perm("user", "organizer", Some("lee@larkspur.example"), None),
            perm("group", "fileOrganizer", Some("it@larkspur.example"), None),
        ];
        let (shares, _) = sharing(&perms, "larkspur.example", &HashMap::new());
        assert_eq!(shares[0]["role"], "writer");
        assert_eq!(
            (shares[1]["id"].as_str(), shares[1]["role"].as_str()),
            (Some("it@larkspur.example"), Some("writer"))
        );
    }

    #[test]
    fn a_file_at_the_top_hangs_under_its_drive_and_keeps_only_its_own_permissions() {
        let file = File {
            id: "f1".into(),
            name: "Board minutes".into(),
            mime_type: "application/vnd.google-apps.document".into(),
            drive_id: Some("d1".into()),
            ..File::default()
        };
        let inherited = Permission {
            permission_details: vec![PermissionDetail { inherited: true }],
            ..perm("group", "reader", Some("everyone@larkspur.example"), None)
        };
        let own = Permission {
            permission_details: vec![PermissionDetail { inherited: true }, PermissionDetail { inherited: false }],
            ..perm("user", "writer", Some("sam@larkspur.example"), None)
        };
        let r = file_resource(&file, &[inherited, own], &cfg("x"), &HashMap::new());
        assert_eq!(
            (r["id"].as_str(), r["parent"].as_str(), r["kind"].as_str()),
            (Some("gdrive:f1"), Some("gdrive:d1"), Some("document"))
        );
        assert_eq!(r["sharing"], json!([{ "kind": "person", "id": "sam@larkspur.example", "role": "writer" }]));
    }

    /// A fake Google (Drive and the Admin directory) and a fake awzy-context, sharing state.
    #[derive(Default)]
    struct Fake {
        files: Vec<Value>,
        changes: Vec<Value>,
        written: Vec<Value>,
        deleted: Vec<String>,
        existing: Vec<String>,
    }

    type Shared = Arc<Mutex<Fake>>;

    async fn serve(fake: Shared) -> String {
        let app = Router::new()
            .route("/drive/v3/drives", get(|| async { Json(json!({ "drives": [{ "id": "d1", "name": "Finance" }] })) }))
            .route(
                "/drive/v3/files/{id}/permissions",
                get(|Path(id): Path<String>| async move {
                    let perms = match id.as_str() {
                        "d1" => json!([{ "type": "group", "role": "organizer", "emailAddress": "finance@larkspur.example" }]),
                        _ => json!([{ "type": "user", "role": "reader", "emailAddress": "auditor@partner.example",
                                      "permissionDetails": [{ "inherited": false }] }]),
                    };
                    Json(json!({ "permissions": perms }))
                }),
            )
            .route("/drive/v3/files", get(|State(f): State<Shared>| async move { Json(json!({ "files": f.lock().unwrap().files })) }))
            .route("/drive/v3/changes/startPageToken", get(|| async { Json(json!({ "startPageToken": "1" })) }))
            .route(
                "/drive/v3/changes",
                get(|State(f): State<Shared>, Query(q): Query<HashMap<String, String>>| async move {
                    assert_eq!(q["pageToken"], "1");
                    Json(json!({ "changes": f.lock().unwrap().changes, "newStartPageToken": "2" }))
                }),
            )
            .route(
                "/admin/directory/v1/groups",
                get(|| async { Json(json!({ "groups": [{ "email": "finance@larkspur.example", "name": "finance" }] })) }),
            )
            .route(
                "/v1/resources/batch",
                post(|State(f): State<Shared>, Json(b): Json<Value>| async move {
                    let rs = b["resources"].as_array().unwrap().clone();
                    f.lock().unwrap().written.extend(rs);
                    Json(json!({}))
                }),
            )
            .route(
                "/v1/resources/{id}",
                delete(|State(f): State<Shared>, Path(id): Path<String>| async move {
                    f.lock().unwrap().deleted.push(id);
                    StatusCode::NO_CONTENT
                }),
            )
            .route(
                "/v1/resources",
                get(|State(f): State<Shared>, Query(q): Query<HashMap<String, String>>| async move {
                    let f = f.lock().unwrap();
                    let page: Vec<Value> = if q.contains_key("after") { vec![] } else { f.existing.iter().map(|id| json!({ "id": id })).collect() };
                    Json(json!({ "resources": page }))
                }),
            )
            .with_state(fake);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn a_crawl_writes_drives_and_files_then_changes_keep_them_current() {
        let fake: Shared = Arc::default();
        {
            let mut f = fake.lock().unwrap();
            f.files = vec![
                json!({ "id": "folder", "name": "Payroll", "mimeType": "application/vnd.google-apps.folder", "parents": ["d1"], "driveId": "d1" }),
                json!({ "id": "sheet", "name": "Salaries", "mimeType": "application/vnd.google-apps.spreadsheet",
                        "parents": ["folder"], "driveId": "d1", "hasAugmentedPermissions": true }),
            ];
            f.existing = vec!["gdrive:d1".into(), "gdrive:folder".into(), "gdrive:old".into()];
        }
        let base = serve(fake.clone()).await;
        let http = Http::default();
        let context = ContextClient::new(&base, http.clone(), awzy_service_auth::TokenSource::off());
        let mut sync = DriveSync::new(cfg(&base), Arc::new(Credentials::Fixed("t".into())), http, context);

        let s = sync.full_crawl().await.unwrap();
        assert_eq!(s, Summary { written: 3, deleted: 1 });
        {
            let f = fake.lock().unwrap();
            let drive = f.written.iter().find(|r| r["id"] == "gdrive:d1").unwrap();
            assert_eq!(
                drive["sharing"],
                json!([{ "kind": "group", "id": "finance", "role": "writer" }]),
                "members by group name"
            );
            let sheet = f.written.iter().find(|r| r["id"] == "gdrive:sheet").unwrap();
            assert_eq!(sheet["parent"], "gdrive:folder");
            assert_eq!(sheet["attributes"]["external_shares"], 1, "shared with a partner's auditor");
            assert_eq!(f.deleted, ["gdrive:old"], "gone from Drive while the connector was down");
        }

        fake.lock().unwrap().changes = vec![
            json!({ "fileId": "folder", "removed": false,
                    "file": { "id": "folder", "name": "Payroll 2026", "mimeType": "application/vnd.google-apps.folder", "parents": ["d1"], "driveId": "d1" } }),
            json!({ "fileId": "sheet", "removed": false, "file": { "id": "sheet", "trashed": true } }),
        ];
        let s = sync.poll_changes().await.unwrap();
        assert_eq!(s, Summary { written: 2, deleted: 1 }, "the drive's members again, and the renamed folder");
        let f = fake.lock().unwrap();
        assert_eq!(f.written.last().unwrap()["name"], "Payroll 2026");
        assert_eq!(f.deleted.last().unwrap(), "gdrive:sheet", "trashed");
        assert_eq!(sync.cursors["d1"], "2");
    }
}

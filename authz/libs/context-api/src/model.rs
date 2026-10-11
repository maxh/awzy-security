//! awzy-context's request and response bodies. Each derives both directions, so the server can
//! write them and clients read them with the same definition.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One connector's settings, as the client portal set them (ADR 0104).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorSettings {
    pub name: String,
    pub enabled: bool,
    /// The connector's own settings, an object; each connector says which fields it takes.
    pub settings: Value,
    /// When the client portal last wrote its credential; the credential isn't here.
    pub credential_set_at: Option<DateTime<Utc>>,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

/// `PUT /v1/connector-settings/{name}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PutConnectorSettings {
    pub enabled: bool,
    pub settings: Value,
    /// The portal wrote a new credential with this change.
    #[serde(default)]
    pub credential_set: bool,
    pub updated_by: String,
}

/// `GET /v1/connector-settings`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorSettingsList {
    pub connectors: Vec<ConnectorSettings>,
}

/// `GET` and `PUT /v1/connector-settings/{name}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorSettingsOne {
    pub connector: ConnectorSettings,
}

/// Who a resource is shared with, as its connector reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Share {
    /// human | group | domain | anyone
    pub kind: String,
    /// A userName, a group name, or a domain; empty for `anyone`.
    #[serde(default)]
    pub id: String,
    /// reader | commenter | writer | owner
    #[serde(default)]
    pub role: String,
}

/// What awzy-classify decided about a resource (ADR 0047).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Classification {
    /// None when nothing placed it yet.
    pub sensitivity: Option<String>,
    /// label | content | metadata
    pub by: Option<String>,
    pub reason: String,
    pub at: DateTime<Utc>,
}

/// A resource as awzy-context keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub source: String,
    pub kind: String,
    pub name: String,
    pub parent: Option<String>,
    pub owner: Option<String>,
    /// The connector's tier; the classifier's may be higher.
    pub sensitivity: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub sharing: Vec<Share>,
    pub text: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub attributes: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub classification: Option<Classification>,
}

/// `GET /v1/resources/{id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceOne {
    pub resource: Resource,
}

/// `GET /v1/resources`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceList {
    pub resources: Vec<Resource>,
}

/// `GET /v1/resources/{id}/entities`: the resource and the folders above it as Cedar entities,
/// nearest first, and who is assigned to work that links it or a folder above it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceEntities {
    /// Cedar entity JSON (`uid`, `attrs`, `parents`), which the PDP reads as-is.
    pub entities: Vec<CedarEntity>,
    #[serde(default)]
    pub assignees: Vec<String>,
}

/// A Cedar entity: its type and ID, and its attributes and parents as Cedar JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CedarEntity {
    pub uid: EntityUid,
    #[serde(default)]
    pub attrs: Value,
    #[serde(default)]
    pub parents: Vec<EntityUid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUid {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
}

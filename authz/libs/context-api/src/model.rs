//! awzy-context's request and response bodies. Each derives both directions, so the server can
//! write them and clients read them with the same definition.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
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
    /// The kinds of data it found (ADR 0123).
    #[serde(default)]
    pub data_types: Vec<DataType>,
}

/// One kind of data the classifier found in a resource (ADR 0123).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataType {
    /// `customer_personal`, `financial`, … or a kind the client sent.
    pub data_type: String,
    /// label | content | metadata
    pub by: String,
    pub reason: String,
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

impl Resource {
    /// The data types a connector or the client's app sent in `attributes.data_types` (ADR 0123).
    pub fn sent_data_types(&self) -> Vec<String> {
        #[derive(Default, Deserialize)]
        struct Sent {
            #[serde(default)]
            data_types: Vec<String>,
        }
        serde_json::from_value::<Sent>(self.attributes.clone()).unwrap_or_default().data_types
    }
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
    /// The open work items that link it, each with who is on it (ADR 0118).
    #[serde(default)]
    pub work: Vec<WorkLink>,
}

/// An open work item that links a resource, and who is assigned to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkLink {
    pub id: String,
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

/// Who an investigation is about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationPrincipal {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
}

/// Something an investigation's finding rests on, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cited {
    pub id: String,
    #[serde(default)]
    pub why: String,
}

/// A list awzy-context may store as null.
fn null_as_empty<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

/// One of awzy-reasoner's investigations, as awzy-context keeps it (ADR 0071).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Investigation {
    pub qid: String,
    pub principal: InvestigationPrincipal,
    pub trigger: String,
    pub floor: i32,
    pub status: String,
    pub verdict: String,
    pub risk: i32,
    pub summary: String,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub evidence: Vec<Cited>,
    pub containment: String,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub notes: Vec<String>,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    /// Who closed it (an actor's typed text, ADR 0060), when, and why; null while open.
    pub closed_by: Option<String>,
    pub closed_at: Option<DateTime<Utc>>,
    pub close_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationOne {
    pub investigation: Investigation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationList {
    pub investigations: Vec<Investigation>,
}

/// Closing an investigation once a person has reviewed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloseInvestigation {
    pub reason: String,
}

/// The principals snapshot the PDPs load: people, groups, and service accounts as Cedar
/// entities, with the groups each is in as its parents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrincipalsSnapshot {
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub entities: Vec<CedarEntity>,
}

/// Tokens spent on investigations over a period.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct InvestigationUsage {
    #[serde(default)]
    pub investigations: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct InvestigationUsageOne {
    pub usage: InvestigationUsage,
}

/// One principal's risk score, as awzy-reasoner writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskScore {
    pub principal: EntityUid,
    pub score: i32,
    #[serde(default)]
    pub reasons: Vec<String>,
    /// For a person: how much their requests look like software using their credentials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automation: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub automation_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskBatch {
    pub scores: Vec<RiskScore>,
}

/// Just the ID of a resource or work item, for listing what a source has written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Id {
    pub id: String,
}

/// A page of a source's resources or work items, read for their IDs only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IdPage {
    pub resources: Vec<Id>,
    pub work_items: Vec<Id>,
}

/// How to reach a person: the email their IdP provisioned, if any (#65).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanContact {
    pub user_name: String,
    pub email: Option<String>,
}

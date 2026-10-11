//! Looking up a principal or a resource as the PDPs see it: the Look up page. The same reads as
//! the MCP endpoint's `get_principal` and `get_resource`, for those who may view the directory.

use async_graphql::{Context, Enum, Object, Result, SimpleObject};
use serde_json::Value;

use crate::{
    access::{require, PortalAction::*, PortalArea},
    directory::context,
    mcp::principal_from_snapshot,
};

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum PrincipalType {
    Human,
    ServiceAccount,
}

impl PrincipalType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Human => "Human",
            Self::ServiceAccount => "ServiceAccount",
        }
    }
}

/// A person or service account as the PDPs see it.
#[derive(SimpleObject)]
pub struct PrincipalDetail {
    /// Whether awzy knows it at all: provisioned by the IdP, or a registered service account.
    pub known: bool,
    /// Its Cedar attributes, as JSON: `active`, a service account's `owner` and `ai`.
    pub attributes: Option<String>,
    /// Every group above it, nested groups included.
    pub groups: Vec<String>,
    /// awzy-reasoner's risk score, 0 to 100, when it has one.
    pub risk_score: Option<i64>,
}

/// One resource awzy-context knows, as a search finds it.
#[derive(SimpleObject)]
pub struct ResourceSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub source: String,
    pub owner: Option<String>,
    /// The connector's tier; the classifier's may be higher (see `resource`).
    pub sensitivity: Option<String>,
}

/// What awzy-classify decided about a resource (ADR 0047).
#[derive(SimpleObject)]
pub struct ResourceClassification {
    /// None when nothing placed it yet.
    pub sensitivity: Option<String>,
    /// label, content, or metadata.
    pub by: Option<String>,
    pub reason: String,
}

/// A resource as the PDPs see it.
#[derive(SimpleObject)]
pub struct ResourceDetail {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub source: String,
    pub owner: Option<String>,
    /// The connector's tier.
    pub sensitivity: Option<String>,
    pub classification: Option<ResourceClassification>,
    /// Who it's shared with, as JSON (the connector's sharing entries).
    pub sharing: String,
    pub labels: Vec<String>,
    pub url: Option<String>,
    /// The folders above it, nearest first, by ID.
    pub folders: Vec<String>,
    /// Who is assigned to open work that links it or a folder above it.
    pub assignees: Vec<String>,
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect()
}

fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

/// A path segment, percent-encoded.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Default)]
pub struct LookupQuery;

#[Object]
impl LookupQuery {
    /// A person or service account as the PDPs see it: attributes, groups (nested ones
    /// included), and risk. For those who may view the directory.
    async fn principal(&self, ctx: &Context<'_>, r#type: PrincipalType, id: String) -> Result<PrincipalDetail> {
        require(ctx, View, PortalArea::Directory).await?;
        let snapshot: Value = context(ctx)?.get("/v1/principals").await?;
        let found = principal_from_snapshot(&snapshot, r#type.as_str(), id.trim());
        Ok(PrincipalDetail {
            known: found.entity.is_some(),
            attributes: found.entity.as_ref().map(|e| e["attrs"].to_string()),
            groups: found.groups,
            risk_score: found.risk_score,
        })
    }

    /// Resources whose ID or name contains `query`, ignoring case. At most 50.
    async fn search_resources(&self, ctx: &Context<'_>, query: String) -> Result<Vec<ResourceSummary>> {
        require(ctx, View, PortalArea::Directory).await?;
        let q = query.trim();
        if q.is_empty() {
            return Ok(vec![]);
        }
        let found: Value = context(ctx)?.get(&format!("/v1/resources?limit=50&q={}", encode(q))).await?;
        Ok(found["resources"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| ResourceSummary {
                id: text(&r["id"]).unwrap_or_default(),
                name: text(&r["name"]).unwrap_or_default(),
                kind: text(&r["kind"]).unwrap_or_default(),
                source: text(&r["source"]).unwrap_or_default(),
                owner: text(&r["owner"]),
                sensitivity: text(&r["sensitivity"]),
            })
            .collect())
    }

    /// One resource: its tiers and the classifier's reason, owner, sharing, the folders above it,
    /// and who is assigned to work that links it. Null when awzy-context doesn't know it.
    async fn resource(&self, ctx: &Context<'_>, id: String) -> Result<Option<ResourceDetail>> {
        require(ctx, View, PortalArea::Directory).await?;
        let client = context(ctx)?;
        let (status, body) = client.call("GET", &format!("/v1/resources/{}", encode(&id)), None, None).await?;
        if status == 404 {
            return Ok(None);
        }
        if status != 200 {
            return Err(format!("awzy-context answered {status}").into());
        }
        let found: Value = serde_json::from_slice(&body)?;
        let r = &found["resource"];
        let (_, body) = client.call("GET", &format!("/v1/resources/{}/entities", encode(&id)), None, None).await?;
        let entities: Value = serde_json::from_slice(&body).unwrap_or_default();
        let folders = entities["entities"]
            .as_array()
            .into_iter()
            .flatten()
            .skip(1)
            .filter_map(|e| text(&e["uid"]["id"]))
            .collect();
        let c = &r["classification"];
        Ok(Some(ResourceDetail {
            id: text(&r["id"]).unwrap_or(id),
            name: text(&r["name"]).unwrap_or_default(),
            kind: text(&r["kind"]).unwrap_or_default(),
            source: text(&r["source"]).unwrap_or_default(),
            owner: text(&r["owner"]),
            sensitivity: text(&r["sensitivity"]),
            classification: c.is_object().then(|| ResourceClassification {
                sensitivity: text(&c["sensitivity"]),
                by: text(&c["by"]),
                reason: text(&c["reason"]).unwrap_or_default(),
            }),
            sharing: r["sharing"].to_string(),
            labels: strings(&r["labels"]),
            url: text(&r["url"]),
            folders,
            assignees: strings(&entities["assignees"]),
        }))
    }
}

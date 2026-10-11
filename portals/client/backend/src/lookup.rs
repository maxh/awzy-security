//! Looking up a principal or a resource as the PDPs see it: the Look up page. The same reads as
//! the MCP endpoint's `get_principal` and `get_resource`, for those who may view the directory.

use async_graphql::{Context, Enum, Object, Result, SimpleObject};
use awzy_portal::{JsonObject, JsonString};
use serde_json::Value;

use crate::{
    access::{require, PortalAction::*, PortalArea},
    directory::{context, context_api},
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
    /// Its Cedar attributes, as JSON: `active`, a service account's `owner` and `ai`. Null when a
    /// policy hides them (ADR 0103).
    pub attributes: Option<JsonObject>,
    /// Every group above it, nested groups included.
    pub groups: Vec<String>,
    /// awzy-reasoner's risk score, 0 to 100, when it has one and no policy hides it (ADR 0103).
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
    /// Who it's shared with, as JSON (the connector's sharing entries). Null when a policy hides
    /// it (ADR 0103).
    pub sharing: Option<JsonString>,
    pub labels: Vec<String>,
    pub url: Option<String>,
    /// The folders above it, nearest first, by ID.
    pub folders: Vec<String>,
    /// Who is assigned to open work that links it or a folder above it.
    pub assignees: Vec<String>,
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
            attributes: found.entity.as_ref().and_then(|e| JsonObject::from_json(e["attrs"].clone())),
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
        let found = context_api(ctx)?.search_resources(q, 50).await?;
        Ok(found
            .into_iter()
            .map(|r| ResourceSummary {
                id: r.id,
                name: r.name,
                kind: r.kind,
                source: r.source,
                owner: r.owner,
                sensitivity: r.sensitivity,
            })
            .collect())
    }

    /// One resource: its tiers and the classifier's reason, owner, sharing, the folders above it,
    /// and who is assigned to work that links it. Null when awzy-context doesn't know it.
    async fn resource(&self, ctx: &Context<'_>, id: String) -> Result<Option<ResourceDetail>> {
        require(ctx, View, PortalArea::Directory).await?;
        let api = context_api(ctx)?;
        let Some(r) = api.resource(&id).await? else { return Ok(None) };
        let entities = api.resource_entities(&id).await?;
        let (folders, assignees) = match entities {
            // The first entity is the resource itself; the rest are the folders above it.
            Some(e) => (e.entities.into_iter().skip(1).map(|f| f.uid.id).collect(), e.assignees),
            None => (vec![], vec![]),
        };
        Ok(Some(ResourceDetail {
            id: r.id,
            name: r.name,
            kind: r.kind,
            source: r.source,
            owner: r.owner,
            sensitivity: r.sensitivity,
            classification: r.classification.map(|c| ResourceClassification {
                sensitivity: c.sensitivity,
                by: c.by,
                reason: c.reason,
            }),
            sharing: Some(JsonString::of(&serde_json::to_value(&r.sharing)?)),
            labels: r.labels,
            url: r.url,
            folders,
            assignees,
        }))
    }
}

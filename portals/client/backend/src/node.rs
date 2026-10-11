//! Relay global object identification. Global IDs are QIDs (`<entity_type>_<uuid>`), so
//! `node(id)` routes on the type the ID parses as.

use async_graphql::{Context, Interface, Object, Result, ID};
use awzy_model::{ChallengeId, ContainmentId, DecisionId, InvestigationId, PolicyBundleId, SecurityKeyId, SupportSessionId};

use crate::{
    challenges::{Challenge, Containment},
    decisions::Decision,
    investigations::Investigation,
    policies::PolicyBundle,
    security_keys::SecurityKey,
    support::SupportSession,
};

#[derive(Interface)]
#[graphql(field(name = "id", ty = "ID"))]
pub enum Node {
    PolicyBundle(PolicyBundle),
    Decision(Decision),
    Challenge(Challenge),
    Containment(Containment),
    Investigation(Investigation),
    SecurityKey(SecurityKey),
    SupportSession(SupportSession),
}

#[derive(Default)]
pub struct NodeQuery;

#[Object]
impl NodeQuery {
    /// Refetches any object by its ID.
    async fn node(&self, ctx: &Context<'_>, id: ID) -> Result<Option<Node>> {
        if let Ok(id) = id.parse::<PolicyBundleId>() {
            return Ok(crate::policies::get_by_id(ctx, id).await?.map(Node::PolicyBundle));
        }
        if let Ok(id) = id.parse::<DecisionId>() {
            return Ok(crate::decisions::get_by_id(ctx, id).await?.map(Node::Decision));
        }
        if let Ok(id) = id.parse::<ChallengeId>() {
            return Ok(crate::challenges::get_by_id(ctx, id).await?.map(Node::Challenge));
        }
        if let Ok(id) = id.parse::<ContainmentId>() {
            return Ok(crate::challenges::containment_by_id(ctx, id).await?.map(Node::Containment));
        }
        if let Ok(id) = id.parse::<InvestigationId>() {
            return Ok(crate::investigations::get_by_id(ctx, id).await?.map(Node::Investigation));
        }
        if let Ok(id) = id.parse::<SecurityKeyId>() {
            return Ok(crate::security_keys::get_by_id(ctx, id).await?.map(Node::SecurityKey));
        }
        if let Ok(id) = id.parse::<SupportSessionId>() {
            return Ok(crate::support::get_by_id(ctx, id).await?.map(Node::SupportSession));
        }
        Ok(None)
    }
}

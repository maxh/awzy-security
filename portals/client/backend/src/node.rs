//! Relay global object identification. Global IDs are QIDs (`<entity_type>_<uuid>`), so
//! `node(id)` routes on the type the ID parses as.

use async_graphql::{Context, Interface, Object, Result, ID};
use awzy_model::{DecisionId, PolicyBundleId};

use crate::{decisions::Decision, policies::PolicyBundle};

#[derive(Interface)]
#[graphql(field(name = "id", ty = "ID"))]
pub enum Node {
    PolicyBundle(PolicyBundle),
    Decision(Decision),
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
        Ok(None)
    }
}

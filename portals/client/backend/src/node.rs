//! Relay global object identification. Global IDs are QIDs (`<entity_type>_<uuid>`), so
//! `node(id)` routes on the entity the QID is of.

use async_graphql::{Context, Interface, Object, Result, ID};
use awzy_model::QidEntity;

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
#[allow(clippy::large_enum_variant, reason = "one is built per node(id) call; boxing buys nothing")]
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
        let Some(entity) = QidEntity::of(&id) else { return Ok(None) };
        // Exhaustive, so a new entity has to say here whether it's a Node in this portal.
        Ok(match entity {
            QidEntity::PolicyBundle => crate::policies::get_by_id(ctx, id.parse()?).await?.map(Node::PolicyBundle),
            QidEntity::Decision => crate::decisions::get_by_id(ctx, id.parse()?).await?.map(Node::Decision),
            QidEntity::Challenge => crate::challenges::get_by_id(ctx, id.parse()?).await?.map(Node::Challenge),
            QidEntity::Containment => {
                crate::challenges::containment_by_id(ctx, id.parse()?).await?.map(Node::Containment)
            }
            QidEntity::Investigation => {
                crate::investigations::get_by_id(ctx, id.parse()?).await?.map(Node::Investigation)
            }
            QidEntity::SecurityKey => crate::security_keys::get_by_id(ctx, id.parse()?).await?.map(Node::SecurityKey),
            QidEntity::SupportSession => crate::support::get_by_id(ctx, id.parse()?).await?.map(Node::SupportSession),
            // Not objects this portal serves.
            QidEntity::Install | QidEntity::Event | QidEntity::Ceremony | QidEntity::Plan => None,
        })
    }
}

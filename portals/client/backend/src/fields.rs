//! Field-level authorization (ADR 0103): the sensitive fields the portal shows, each checked with
//! the PDP as `view` on `PortalField::"<Type>.<field>"`, a child of its area. Base policy permits
//! every field to whoever sees its area; a client hides one with a forbid. A hidden field
//! resolves to null, and the rest of the page still loads, so each of these is nullable.

use std::{collections::HashMap, sync::Arc};

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextResolve, ResolveInfo},
    ServerResult, Value,
};
use tokio::sync::Mutex;

use crate::{
    access::{self, PortalArea, Surface, Viewer},
    Services,
};

/// (GraphQL type, field, the area it's in). The MCP endpoint hides the same, by these names.
pub(crate) const SENSITIVE: &[(&str, &str, PortalArea)] = &[
    ("Decision", "context", PortalArea::Decisions),
    ("PrincipalDetail", "attributes", PortalArea::Directory),
    ("PrincipalDetail", "riskScore", PortalArea::Directory),
    ("ResourceDetail", "sharing", PortalArea::Directory),
    ("Challenge", "justification", PortalArea::Challenges),
];

/// The field's ID as a `PortalField`, e.g. `Decision.context`, and its area, if it's sensitive.
pub(crate) fn sensitive(type_name: &str, field: &str) -> Option<(String, PortalArea)> {
    SENSITIVE.iter().find(|(t, f, _)| *t == type_name && *f == field).map(|(t, f, area)| (format!("{t}.{f}"), *area))
}

/// One request's answers, so a list of fifty decisions asks about `Decision.context` once.
#[derive(Default)]
pub(crate) struct Visible(Mutex<HashMap<String, bool>>);

impl Visible {
    /// Whether the viewer may see `field`, asking the PDP the first time in this request.
    pub(crate) async fn get(
        &self,
        services: &Services,
        viewer: &Viewer,
        surface: Surface,
        field: &str,
        area: PortalArea,
    ) -> Result<bool, access::AccessError> {
        // Held across the check, so concurrent resolvers of one field wait for the one answer.
        let mut known = self.0.lock().await;
        if let Some(&v) = known.get(field) {
            return Ok(v);
        }
        let v = access::field_visible(services, viewer, surface, area, field).await?;
        known.insert(field.to_string(), v);
        Ok(v)
    }
}

/// Resolves each sensitive field only if the viewer may see it; otherwise null.
pub struct FieldAuthz;

impl ExtensionFactory for FieldAuthz {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(PerRequest(Visible::default()))
    }
}

struct PerRequest(Visible);

#[async_trait::async_trait]
impl Extension for PerRequest {
    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        let Some((field, area)) = sensitive(info.parent_type, info.name) else { return next.run(ctx, info).await };
        // Without a viewer (the schema printer, tests without backends) there's nothing to check.
        let (Some(viewer), Some(services)) = (ctx.data_opt::<Viewer>(), ctx.data_opt::<Services>()) else {
            return next.run(ctx, info).await;
        };
        match self.0.get(services, viewer, Surface::Portal, &field, area).await {
            Ok(true) => next.run(ctx, info).await,
            Ok(false) => Ok(None),
            Err(e) => Err(async_graphql::ServerError::new(e.to_string(), None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{human, pdp, pdp_with};

    #[tokio::test]
    async fn a_field_is_seen_unless_a_policy_hides_it() {
        let alice = human("alice", "security");
        let open = Services { pdp: Some(pdp().await), ..Default::default() };
        assert!(Visible::default()
            .get(&open, &alice, Surface::Portal, "Decision.context", PortalArea::Decisions)
            .await
            .unwrap());

        let hide = format!(
            "{}\nforbid (principal, action == Action::\"view\", resource == PortalField::\"Decision.context\")\n\
             unless {{ principal in Group::\"privacy\" }};",
            awzy_pdp::DEFAULT_POLICIES
        );
        let hiding = Services { pdp: Some(pdp_with(&hide).await), ..Default::default() };
        let visible = Visible::default();
        assert!(!visible
            .get(&hiding, &alice, Surface::Portal, "Decision.context", PortalArea::Decisions)
            .await
            .unwrap());
        assert!(
            visible
                .get(&hiding, &alice, Surface::Portal, "PrincipalDetail.attributes", PortalArea::Directory)
                .await
                .unwrap(),
            "a forbid on one field leaves the others"
        );
        let privacy = human("pat", "privacy");
        assert!(Visible::default()
            .get(&hiding, &privacy, Surface::Portal, "Decision.context", PortalArea::Decisions)
            .await
            .unwrap());
    }

    #[test]
    fn sensitive_fields_are_named_by_type_and_field() {
        assert_eq!(sensitive("Decision", "context"), Some(("Decision.context".to_string(), PortalArea::Decisions)));
        assert_eq!(sensitive("Decision", "verdict"), None);
        assert_eq!(sensitive("PrincipalDetail", "riskScore").unwrap().0, "PrincipalDetail.riskScore");
    }
}

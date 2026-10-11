//! Every mutation, recorded (platform plan's day-one guards; ADR 0084's activity log).
//!
//! Each service already keeps who made the changes it owns: who published a bundle, switched a
//! mode, decided a challenge, closed an investigation. This adds one trail across all of them:
//! each GraphQL mutation field the portal runs is recorded as the viewer's activity in
//! awzy-intake, with whether it succeeded, like the MCP endpoint's tool calls. Best effort, as
//! that log is: recording never slows or fails the change.

use std::sync::{Arc, Mutex};

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextExecute, NextParseQuery},
    parser::types::{ExecutableDocument, OperationType, Selection},
    Response, ServerResult, Variables,
};

use crate::{access::Viewer, Services};

pub struct MutationAudit;

impl ExtensionFactory for MutationAudit {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(Audit { fields: Mutex::new(vec![]) })
    }
}

/// One request's: the mutation fields it runs, once parsed.
struct Audit {
    fields: Mutex<Vec<String>>,
}

/// The top-level fields of the mutation `operation_name` picks (or the only operation).
fn mutation_fields(doc: &ExecutableDocument, operation_name: Option<&str>) -> Vec<String> {
    doc.operations
        .iter()
        .filter(|(name, _)| operation_name.is_none() || name.map(|n| n.as_str()) == operation_name)
        .filter(|(_, op)| op.node.ty == OperationType::Mutation)
        .flat_map(|(_, op)| {
            op.node.selection_set.node.items.iter().filter_map(|s| match &s.node {
                Selection::Field(f) => Some(f.node.name.node.to_string()),
                _ => None,
            })
        })
        .collect()
}

#[async_trait::async_trait]
impl Extension for Audit {
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let doc = next.run(ctx, query, variables).await?;
        *self.fields.lock().expect("audit lock") = mutation_fields(&doc, None);
        Ok(doc)
    }

    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let res = next.run(ctx, operation_name).await;
        let fields = std::mem::take(&mut *self.fields.lock().expect("audit lock"));
        if fields.is_empty() {
            return res;
        }
        let (Some(viewer), Some(services)) = (ctx.data_opt::<Viewer>(), ctx.data_opt::<Services>()) else { return res };
        let outcome = if res.errors.is_empty() { "success" } else { "failure" };
        for field in fields {
            tracing::info!(actor = %viewer.actor().label(), mutation = %field, outcome, "portal mutation");
            let activity =
                awzy_activity::Activity::now(viewer.subject.clone(), viewer.on_behalf_of.clone(), &field, None)
                    .with_outcome(Some(outcome))
                    .with_attribute("client", "portal")
                    .with_attribute("operation", operation_name.unwrap_or("anonymous"));
            services.activity.record(activity);
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_mutations_top_level_fields_are_recorded() {
        let doc = async_graphql::parser::parse_query(
            "mutation Publish { publishPolicyBundle(input: {id: \"x\"}) { id } setEnforcementMode(input: {mode: ENFORCE}) { challengeQid } } \
             query Look { enforcement { mode } }",
        )
        .unwrap();
        assert_eq!(mutation_fields(&doc, None), ["publishPolicyBundle", "setEnforcementMode"]);
        let query = async_graphql::parser::parse_query("query { enforcement { mode } }").unwrap();
        assert!(mutation_fields(&query, None).is_empty());
    }
}

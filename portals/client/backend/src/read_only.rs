//! A portal that changes nothing (AWZY_PORTAL_READ_ONLY): a public demo, where anyone can sign
//! in as a sample person, so one visitor's changes never reach the next. Every GraphQL mutation
//! is refused before it runs, as are the MCP tools that change something. The exception is
//! asking Awzy Bot, which changes nothing there: it can't save a draft, and it's rate limited.

use std::sync::Arc;

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextParseQuery},
    parser::types::{ExecutableDocument, OperationType, Selection},
    Pos, ServerResult, Variables,
};

/// The mutations a read-only portal still runs, because they change nothing there.
const ALLOWED: &[&str] = &["askAwzyBot"];

/// What a refused change says.
pub const REFUSED: &str =
    "This is a public demo: you can look at everything, but changes are turned off. Run your own with `tools/demo up`.";

pub struct ReadOnly;

impl ExtensionFactory for ReadOnly {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(ReadOnly)
    }
}

#[async_trait::async_trait]
impl Extension for ReadOnly {
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let doc = next.run(ctx, query, variables).await?;
        let changes = |items: &[async_graphql::Positioned<Selection>]| {
            items.iter().any(|item| match &item.node {
                Selection::Field(f) => !ALLOWED.contains(&f.node.name.node.as_str()),
                // A fragment could hide any field; nothing the portal's pages send uses one here.
                _ => true,
            })
        };
        if doc
            .operations
            .iter()
            .any(|(_, op)| op.node.ty == OperationType::Mutation && changes(&op.node.selection_set.node.items))
        {
            return Err(awzy_portal::ErrorCode::Forbidden.error(REFUSED).into_server_error(Pos::default()));
        }
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use crate::{schema_with, Services};

    #[tokio::test]
    async fn a_read_only_portal_refuses_every_mutation_and_still_answers_queries() {
        let schema = schema_with(Services { read_only: true, ..Services::default() });
        let res = schema.execute("mutation { setEnforcementMode(mode: ENFORCE) { __typename } }").await;
        assert_eq!(res.errors.len(), 1, "{res:?}");
        assert!(res.errors[0].message.contains("changes are turned off"));
        let code = res.errors[0].extensions.as_ref().and_then(|e| e.get("code")).map(|c| c.to_string());
        assert_eq!(code.as_deref(), Some("\"FORBIDDEN\""));

        let res = schema.execute("{ apiVersion }").await;
        assert!(res.errors.is_empty(), "{res:?}");

        // Asking Awzy Bot gets past it (and fails here only because no bot is set up), but not
        // alongside a change.
        let ask = schema.execute("mutation { askAwzyBot(input: { message: \"hi\" }) { reply } }").await;
        assert!(ask.errors.iter().all(|e| !e.message.contains("changes are turned off")), "{ask:?}");
        let both = schema
            .execute("mutation { askAwzyBot(input: { message: \"hi\" }) { reply } setEnforcementMode(mode: ENFORCE) { __typename } }")
            .await;
        assert!(both.errors[0].message.contains("changes are turned off"), "{both:?}");
    }

    #[tokio::test]
    async fn a_portal_that_isnt_read_only_runs_mutations_as_before() {
        let schema = schema_with(Services::default());
        let res = schema.execute("mutation { setEnforcementMode(mode: ENFORCE) { __typename } }").await;
        assert!(res.errors.iter().all(|e| !e.message.contains("changes are turned off")), "{res:?}");
    }
}

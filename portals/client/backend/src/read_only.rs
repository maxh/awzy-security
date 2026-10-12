//! A portal that changes nothing (AWZY_PORTAL_READ_ONLY): a public demo, where anyone can sign
//! in as a sample person, so one visitor's changes never reach the next. Every GraphQL mutation
//! is refused before it runs, as are the MCP tools that change something.

use std::sync::Arc;

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextParseQuery},
    parser::types::{ExecutableDocument, OperationType},
    Pos, ServerResult, Variables,
};

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
        if doc.operations.iter().any(|(_, op)| op.node.ty == OperationType::Mutation) {
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
    }

    #[tokio::test]
    async fn a_portal_that_isnt_read_only_runs_mutations_as_before() {
        let schema = schema_with(Services::default());
        let res = schema.execute("mutation { setEnforcementMode(mode: ENFORCE) { __typename } }").await;
        assert!(res.errors.iter().all(|e| !e.message.contains("changes are turned off")), "{res:?}");
    }
}

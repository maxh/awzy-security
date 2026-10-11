//! Persisted queries (ADR 0088, 0090): each frontend sends the ID the Relay compiler gave its
//! operation, the SHA-256 of its text, and the backend runs the text from the built frontend it
//! serves, which carries them as `persisted_queries.json`. With `only`, as the
//! client portal in a client's install, it runs nothing else, so a signed-in user can't send it
//! queries of their own.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextPrepareRequest},
    Request, ServerError, ServerResult, Value,
};
use sha2::{Digest, Sha256};

/// The schema extension that resolves persisted query IDs, and refuses anything else when `only`.
#[derive(Clone)]
pub struct PersistedQueries {
    queries: Arc<HashMap<String, String>>,
    only: bool,
}

impl PersistedQueries {
    /// From the Relay compiler's `persisted_queries.json`. With `only`, query text that isn't one
    /// of them is refused; without, as when run locally or for GraphiQL, it runs too.
    pub fn new(json: &str, only: bool) -> Result<Self, serde_json::Error> {
        Ok(Self { queries: Arc::new(serde_json::from_str(json)?), only })
    }

    /// From the built frontend in `dir` (`AWZY_PORTAL_STATIC_DIR`), which carries its queries as
    /// `persisted_queries.json`. Without one, as when run locally against the Vite dev server,
    /// which sends query text, there are none; then `only` would refuse everything, so it's an
    /// error.
    pub fn from_frontend(dir: Option<&Path>, only: bool) -> Result<Self, PersistedQueriesError> {
        let Some(dir) = dir else {
            return if only { Err(PersistedQueriesError::NoFrontend) } else { Ok(Self::none()) };
        };
        let path = dir.join("persisted_queries.json");
        let json = std::fs::read_to_string(&path).map_err(|e| PersistedQueriesError::Read(path.clone(), e))?;
        Self::new(&json, only).map_err(|e| PersistedQueriesError::Parse(path, e))
    }

    /// No persisted queries, and query text runs: for tests.
    pub fn none() -> Self {
        Self { queries: Arc::default(), only: false }
    }

    fn resolve(&self, mut request: Request) -> ServerResult<Request> {
        if let Some(persisted) = request.extensions.remove("persistedQuery") {
            let id = match &persisted {
                Value::Object(o) => o.get("sha256Hash").and_then(|v| match v {
                    Value::String(s) => Some(s.as_str()),
                    _ => None,
                }),
                _ => None,
            };
            let id = id.ok_or_else(|| ServerError::new("persistedQuery needs a sha256Hash", None))?;
            let text = self.queries.get(id).ok_or_else(|| {
                // Most likely a page loaded before an upgrade, asking for a query this release dropped.
                ServerError::new("this portal doesn't know that query; reload the page", None)
            })?;
            request.query = text.clone();
            return Ok(request);
        }
        if self.only && !self.queries.contains_key(&format!("{:x}", Sha256::digest(request.query.as_bytes()))) {
            return Err(ServerError::new("this portal runs only its own pages' queries", None));
        }
        Ok(request)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PersistedQueriesError {
    #[error("running only persisted queries needs the built frontend that carries them (AWZY_PORTAL_STATIC_DIR)")]
    NoFrontend,
    #[error("reading {0}: {1}")]
    Read(PathBuf, std::io::Error),
    #[error("parsing {0}: {1}")]
    Parse(PathBuf, serde_json::Error),
}

impl ExtensionFactory for PersistedQueries {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(self.clone())
    }
}

#[async_trait::async_trait]
impl Extension for PersistedQueries {
    async fn prepare_request(
        &self,
        ctx: &ExtensionContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        next.run(ctx, self.resolve(request)?).await
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::{EmptyMutation, EmptySubscription, Object, Schema};

    use super::*;

    struct Query;

    #[Object]
    impl Query {
        async fn hello(&self) -> &str {
            "hi"
        }
        async fn secret(&self) -> &str {
            "no"
        }
    }

    const HELLO: &str = "query HelloQuery {\n  hello\n}\n";

    fn schema(only: bool) -> Schema<Query, EmptyMutation, EmptySubscription> {
        let id = format!("{:x}", Sha256::digest(HELLO.as_bytes()));
        let json = serde_json::json!({ id: HELLO }).to_string();
        Schema::build(Query, EmptyMutation, EmptySubscription)
            .extension(PersistedQueries::new(&json, only).unwrap())
            .finish()
    }

    fn by_id(id: &str) -> Request {
        let mut request = Request::new("");
        request.extensions.insert(
            "persistedQuery".into(),
            Value::from_json(serde_json::json!({ "version": 1, "sha256Hash": id })).unwrap(),
        );
        request
    }

    #[tokio::test]
    async fn a_built_portal_runs_only_its_own_queries() {
        let id = format!("{:x}", Sha256::digest(HELLO.as_bytes()));
        let schema = schema(true);
        let res = schema.execute(by_id(&id)).await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        assert_eq!(res.data.to_string(), r#"{hello: "hi"}"#);
        // Its own query's text is fine too; anything else isn't.
        assert!(schema.execute(HELLO).await.errors.is_empty());
        let res = schema.execute("{ secret }").await;
        assert_eq!(res.errors[0].message, "this portal runs only its own pages' queries");
        let res = schema.execute(by_id("0000")).await;
        assert!(res.errors[0].message.contains("reload the page"));
    }

    #[tokio::test]
    async fn run_locally_it_also_takes_query_text() {
        let res = schema(false).execute("{ secret }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
    }

    #[test]
    fn the_queries_come_from_the_built_frontend() {
        let dir = std::env::temp_dir().join(format!("awzy-persisted-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let id = format!("{:x}", Sha256::digest(HELLO.as_bytes()));
        std::fs::write(dir.join("persisted_queries.json"), serde_json::json!({ id.clone(): HELLO }).to_string())
            .unwrap();
        let loaded = PersistedQueries::from_frontend(Some(&dir), true).unwrap();
        assert_eq!(loaded.queries.get(&id).map(String::as_str), Some(HELLO));
        // Run locally, without a built frontend, there are none, so enforcing them is refused.
        assert!(PersistedQueries::from_frontend(None, false).unwrap().queries.is_empty());
        assert!(matches!(PersistedQueries::from_frontend(None, true), Err(PersistedQueriesError::NoFrontend)));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

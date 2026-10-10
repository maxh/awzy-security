//! Shared by both portal backends: GraphQL routing, serving the built frontend, and later
//! auth and pagination.

use std::path::{Path, PathBuf};

mod scalars;

pub use scalars::UtcTimestamp;

use async_graphql::{http::GraphiQLSource, ObjectType, Schema, SubscriptionType};
use async_graphql_axum::GraphQL;
use axum::{response::Html, routing::get, Router};
use tower_http::services::{ServeDir, ServeFile};

/// Serves `schema` at `/graphql`: POST for queries, GET for GraphiQL during development.
pub fn graphql_router<Q, M, S>(schema: Schema<Q, M, S>) -> Router
where
    Q: ObjectType + 'static,
    M: ObjectType + 'static,
    S: SubscriptionType + 'static,
{
    Router::new().route(
        "/graphql",
        get(|| async { Html(GraphiQLSource::build().endpoint("/graphql").finish()) })
            .post_service(GraphQL::new(schema)),
    )
}

/// Directory holding the built frontend, from `AWZY_PORTAL_STATIC_DIR`. The portal images
/// set it; local development leaves it unset and uses the Vite dev server instead.
pub fn frontend_dir_from_env() -> Option<PathBuf> {
    std::env::var_os("AWZY_PORTAL_STATIC_DIR").map(PathBuf::from)
}

/// Serves the built frontend from `dir` for every path no other route claims.
/// Paths that are not files get `index.html`, so client-side routes survive a reload.
pub fn with_frontend(app: Router, dir: &Path) -> Router {
    let index = ServeFile::new(dir.join("index.html"));
    app.fallback_service(ServeDir::new(dir).fallback(index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    async fn get_body(app: Router, path: &str) -> (u16, String) {
        let res = app.oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
        let status = res.status().as_u16();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn serves_assets_and_falls_back_to_index() {
        let dir = std::env::temp_dir().join(format!("awzy-portal-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), "<html>portal</html>").unwrap();
        std::fs::write(dir.join("assets/app.js"), "console.log(1)").unwrap();

        let app = || with_frontend(Router::new().route("/api", get(|| async { "api" })), &dir);
        assert_eq!(get_body(app(), "/").await, (200, "<html>portal</html>".into()));
        assert_eq!(get_body(app(), "/assets/app.js").await, (200, "console.log(1)".into()));
        assert_eq!(get_body(app(), "/policies/123").await, (200, "<html>portal</html>".into()));
        assert_eq!(get_body(app(), "/api").await, (200, "api".into()));

        std::fs::remove_dir_all(dir).unwrap();
    }
}

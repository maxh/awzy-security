//! Shared by both portal backends: GraphQL routing, serving the built frontend, and later
//! auth and pagination.

use std::path::{Path, PathBuf};

mod actor;
mod scalars;

pub use actor::Actor;
pub use scalars::UtcTimestamp;

/// The deepest query either portal runs. Deep enough for the frontends' Relay queries and
/// GraphiQL's introspection; a query that nests further (`Actor.onBehalfOf` recurses) is refused
/// before it runs.
pub const MAX_QUERY_DEPTH: usize = 16;

/// The most fields one query may ask for, counting each alias. The largest page's query asks for
/// about a hundred; this stops one request fanning out into thousands of resolver calls.
pub const MAX_QUERY_COMPLEXITY: usize = 1_000;

use async_graphql::{http::GraphiQLSource, ObjectType, Schema, SubscriptionType};
use async_graphql_axum::GraphQL;
use axum::{
    extract::Request,
    http::{header, HeaderValue},
    middleware::{self, Next},
    response::{Html, Response},
    routing::get,
    Router,
};
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
    let frontend =
        Router::new().fallback_service(ServeDir::new(dir).fallback(index)).layer(middleware::from_fn(cache_policy));
    app.fallback_service(frontend)
}

/// Vite names everything under `/assets/` by its content hash, so those never change and are
/// cached for good. Everything else is `index.html`, which must be fetched again on every load,
/// or a browser keeps running the previous release. Bazel gives every built file the same
/// modification time, and `index.html` stays the same size from release to release, so
/// ServeDir's ETag (time and size) wouldn't change either: it's removed, and so are the
/// browser's conditional headers, including ones for an ETag cached before this.
async fn cache_policy(mut req: Request, next: Next) -> Response {
    let hashed = req.uri().path().starts_with("/assets/");
    if !hashed {
        req.headers_mut().remove(header::IF_NONE_MATCH);
        req.headers_mut().remove(header::IF_MODIFIED_SINCE);
    }
    let mut res = next.run(req).await;
    let found = res.status().is_success();
    let headers = res.headers_mut();
    if hashed && found {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=31536000, immutable"));
    } else {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        headers.remove(header::ETAG);
        headers.remove(header::LAST_MODIFIED);
    }
    res
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

    #[tokio::test]
    async fn every_load_gets_the_current_index_and_assets_are_cached_for_good() {
        let dir = std::env::temp_dir().join(format!("awzy-portal-cache-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), "<html>portal</html>").unwrap();
        std::fs::write(dir.join("assets/app-3f1a.js"), "console.log(1)").unwrap();
        let app = || with_frontend(Router::new(), &dir);
        let get = |path: &str, etag: Option<&str>| {
            let mut req = Request::get(path);
            if let Some(etag) = etag {
                req = req
                    .header(header::IF_NONE_MATCH, etag)
                    .header(header::IF_MODIFIED_SINCE, "Sat, 01 Jan 2000 00:00:00 GMT");
            }
            req.body(Body::empty()).unwrap()
        };

        for path in ["/", "/installs/install_1"] {
            let res = app().oneshot(get(path, None)).await.unwrap();
            assert_eq!(res.headers()[header::CACHE_CONTROL], "no-cache", "{path}");
            assert!(res.headers().get(header::ETAG).is_none() && res.headers().get(header::LAST_MODIFIED).is_none());
            // A browser holding an ETag from an earlier release still gets the page, not a 304.
            let res = app().oneshot(get(path, Some("\"386d4380.00000000-4c6\""))).await.unwrap();
            assert_eq!(res.status(), 200, "{path}");
        }
        let res = app().oneshot(get("/assets/app-3f1a.js", None)).await.unwrap();
        assert_eq!(res.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");

        std::fs::remove_dir_all(dir).unwrap();
    }
}

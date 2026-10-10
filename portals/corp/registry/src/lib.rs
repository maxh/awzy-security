//! `awzy-registry`: the pull-only proxy at registry.awzysecurity.com (ADR 0028).
//!
//! Installs pull images, signatures, and the chart with HTTP Basic auth: the install ID as the
//! username and its pull token as the password. Only GET and HEAD on `awzy/*` repositories are
//! allowed. Manifests come from ECR through the proxy; layer downloads follow ECR's redirect to
//! a presigned S3 URL, so the bytes never pass through here.

pub mod sigv4;
pub mod tokens;
pub mod upstream;

use std::sync::Arc;

use awzy_model::InstallId;
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};

/// Checks an install's pull token.
pub trait Tokens: Send + Sync + 'static {
    fn check(&self, install: InstallId, token: &str) -> impl std::future::Future<Output = anyhow::Result<bool>> + Send;
}

impl Tokens for tokens::TokenStore {
    async fn check(&self, install: InstallId, token: &str) -> anyhow::Result<bool> {
        Ok(tokens::TokenStore::check(self, install, token).await?)
    }
}

pub struct Registry<T> {
    pub tokens: T,
    pub upstream: upstream::Upstream,
    /// Shown in the Basic auth challenge, e.g. `registry.awzysecurity.com`.
    pub realm: String,
}

pub fn router<T: Tokens>(registry: Registry<T>) -> Router {
    Router::new()
        .route("/v2", any(handle::<T>))
        .route("/v2/", any(handle::<T>))
        .route("/v2/{*rest}", any(handle::<T>))
        .with_state(Arc::new(registry))
}

/// What a path under /v2/ asks for.
#[derive(Debug, PartialEq)]
enum Target<'a> {
    Base,
    Repo { name: &'a str },
}

/// The repository name, if the path is a pull API the proxy serves.
fn parse(path: &str) -> Option<Target<'_>> {
    let rest = path.strip_prefix("/v2")?;
    if rest.is_empty() || rest == "/" {
        return Some(Target::Base);
    }
    let rest = rest.strip_prefix('/')?;
    let name = ["/manifests/", "/blobs/", "/referrers/"]
        .iter()
        .find_map(|sep| rest.find(sep).map(|i| &rest[..i]))
        .or_else(|| rest.strip_suffix("/tags/list"))?;
    let allowed = name.starts_with("awzy/")
        && name.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
        });
    allowed.then_some(Target::Repo { name })
}

fn error(status: StatusCode, code: &str, message: &str) -> Response {
    let body = serde_json::json!({ "errors": [{ "code": code, "message": message }] });
    (status, [(header::CONTENT_TYPE, "application/json")], body.to_string()).into_response()
}

fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Basic ")?;
    let decoded = String::from_utf8(STANDARD.decode(v.trim()).ok()?).ok()?;
    let (user, pass) = decoded.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

async fn handle<T: Tokens>(State(reg): State<Arc<Registry<T>>>, req: Request) -> Response {
    let unauthorized = || {
        let mut r = error(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "sign in with the install ID and its pull token");
        let challenge = format!("Basic realm=\"{}\"", reg.realm);
        r.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_str(&challenge).expect("realm is ASCII"));
        r
    };
    let Some((user, token)) = basic_credentials(req.headers()) else { return unauthorized() };
    let Ok(install) = user.parse::<InstallId>() else { return unauthorized() };
    match reg.tokens.check(install, &token).await {
        Ok(true) => {}
        Ok(false) => return unauthorized(),
        Err(e) => {
            tracing::error!(error = %e, "checking a pull token failed");
            return error(StatusCode::SERVICE_UNAVAILABLE, "UNAVAILABLE", "try again");
        }
    }

    if req.method() != Method::GET && req.method() != Method::HEAD {
        return error(StatusCode::METHOD_NOT_ALLOWED, "UNSUPPORTED", "this registry is pull-only");
    }
    let path = req.uri().path();
    let target = match parse(path) {
        Some(t) => t,
        None => return error(StatusCode::NOT_FOUND, "NAME_UNKNOWN", "only awzy/* repositories are served"),
    };
    if target == Target::Base {
        return (StatusCode::OK, [("docker-distribution-api-version", "registry/2.0")], "{}").into_response();
    }
    let path_and_query = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or(path);
    match reg.upstream.forward(req.method().clone(), path_and_query, req.headers().get(header::ACCEPT)).await {
        Ok(resp) => {
            if let Target::Repo { name } = target {
                tracing::info!(%install, repo = name, method = %req.method(), status = resp.status().as_u16(), "pull");
            }
            resp
        }
        Err(e) => {
            tracing::error!(error = %e, "upstream registry request failed");
            error(StatusCode::BAD_GATEWAY, "UNAVAILABLE", "the release registry is unavailable")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[test]
    fn serves_only_pull_paths_of_awzy_repositories() {
        assert_eq!(parse("/v2/"), Some(Target::Base));
        assert_eq!(parse("/v2/awzy/awzy-pdp/manifests/abc"), Some(Target::Repo { name: "awzy/awzy-pdp" }));
        assert_eq!(parse("/v2/awzy/charts/awzy/manifests/0.1.0-gabc"), Some(Target::Repo { name: "awzy/charts/awzy" }));
        assert_eq!(parse("/v2/awzy/awzy-pdp/blobs/sha256:00"), Some(Target::Repo { name: "awzy/awzy-pdp" }));
        assert_eq!(parse("/v2/awzy/awzy-pdp/tags/list"), Some(Target::Repo { name: "awzy/awzy-pdp" }));
        assert_eq!(parse("/v2/awzy/awzy-pdp/referrers/sha256:00"), Some(Target::Repo { name: "awzy/awzy-pdp" }));
        assert_eq!(parse("/v2/other/image/manifests/x"), None);
        assert_eq!(parse("/v2/awzy/../other/manifests/x"), None);
        assert_eq!(parse("/v2/awzy/x/blobs/uploads/"), Some(Target::Repo { name: "awzy/x" }), "rejected by method");
        assert_eq!(parse("/v2/_catalog"), None);
    }

    struct OneToken(InstallId);
    impl Tokens for OneToken {
        async fn check(&self, install: InstallId, token: &str) -> anyhow::Result<bool> {
            Ok(install == self.0 && token == "awzy_pull_good")
        }
    }

    /// A fake ECR: a manifest, and a blob that redirects like ECR does.
    async fn fake_upstream() -> String {
        let app = Router::new()
            .route(
                "/v2/awzy/awzy-pdp/manifests/v1",
                get(|headers: HeaderMap| async move {
                    assert_eq!(headers.get(header::AUTHORIZATION).unwrap(), "Basic upstream");
                    (
                        [
                            ("content-type", "application/vnd.oci.image.index.v1+json"),
                            ("docker-content-digest", "sha256:aa"),
                        ],
                        "{\"manifests\":[]}",
                    )
                }),
            )
            .route(
                "/v2/awzy/awzy-pdp/blobs/sha256:bb",
                get(|| async { (StatusCode::TEMPORARY_REDIRECT, [("location", "https://s3.example/bb?sig=1")]) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    async fn app() -> (Router, InstallId) {
        let install = InstallId::new();
        let upstream = upstream::Upstream {
            base_url: fake_upstream().await,
            client: upstream::http_client(),
            auth: upstream::Auth::Fixed(Some("Basic upstream".into())),
        };
        (router(Registry { tokens: OneToken(install), upstream, realm: "registry.test".into() }), install)
    }

    fn get_req(path: &str, user: &str, pass: &str) -> axum::http::Request<Body> {
        let auth = format!("Basic {}", STANDARD.encode(format!("{user}:{pass}")));
        axum::http::Request::get(path).header(header::AUTHORIZATION, auth).body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn challenges_without_valid_credentials() {
        let (app, install) = app().await;
        let resp = app.clone().oneshot(axum::http::Request::get("/v2/").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(resp.headers()[header::WWW_AUTHENTICATE], "Basic realm=\"registry.test\"");
        let resp = app.clone().oneshot(get_req("/v2/", &install.to_string(), "awzy_pull_bad")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let resp = app.oneshot(get_req("/v2/", &InstallId::new().to_string(), "awzy_pull_good")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "the token belongs to another install");
    }

    #[tokio::test]
    async fn proxies_manifests_and_passes_blob_redirects_through() {
        let (app, install) = app().await;
        let user = install.to_string();

        let resp = app.clone().oneshot(get_req("/v2/", &user, "awzy_pull_good")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp =
            app.clone().oneshot(get_req("/v2/awzy/awzy-pdp/manifests/v1", &user, "awzy_pull_good")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers()["docker-content-digest"], "sha256:aa");
        assert_eq!(resp.into_body().collect().await.unwrap().to_bytes(), "{\"manifests\":[]}");

        let resp =
            app.clone().oneshot(get_req("/v2/awzy/awzy-pdp/blobs/sha256:bb", &user, "awzy_pull_good")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(resp.headers()["location"], "https://s3.example/bb?sig=1");

        let resp = app.clone().oneshot(get_req("/v2/other/x/manifests/v1", &user, "awzy_pull_good")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let put = axum::http::Request::put("/v2/awzy/awzy-pdp/manifests/v2")
            .header(header::AUTHORIZATION, format!("Basic {}", STANDARD.encode(format!("{user}:awzy_pull_good"))))
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.oneshot(put).await.unwrap().status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}

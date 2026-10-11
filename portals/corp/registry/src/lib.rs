//! `awzy-registry`: the pull-only proxy at registry.awzysecurity.com (ADR 0028).
//!
//! Installs pull images, signatures, and the chart with HTTP Basic auth: the install ID as the
//! username and its pull token as the password. Only GET and HEAD on `awzy/*` repositories are
//! allowed. Manifests come from ECR through the proxy; layer downloads follow ECR's redirect to
//! a presigned S3 URL, so the bytes never pass through here.
//!
//! The chart is how an install follows its release channel (ADR 0094): the proxy lists only the
//! chart version the install's channel names, and refuses any other, so Argo CD, asking for the
//! newest version in a range, gets that one.

pub mod channels;
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

/// Which chart version an install may pull.
pub trait Channels: Send + Sync + 'static {
    fn chart_version(
        &self,
        install: InstallId,
    ) -> impl std::future::Future<Output = anyhow::Result<Option<String>>> + Send;
}

impl Channels for channels::ChannelStore {
    async fn chart_version(&self, install: InstallId) -> anyhow::Result<Option<String>> {
        Ok(channels::ChannelStore::chart_version(self, install).await?)
    }
}

/// The chart's repository: the one the proxy shows each install a version of.
pub const CHART_REPO: &str = "awzy/charts/awzy";

pub struct Registry<T, C> {
    pub tokens: T,
    pub channels: C,
    pub upstream: upstream::Upstream,
    /// Shown in the Basic auth challenge, e.g. `registry.awzysecurity.com`.
    pub realm: String,
}

pub fn router<T: Tokens, C: Channels>(registry: Registry<T, C>) -> Router {
    Router::new()
        .route("/v2", any(handle::<T, C>))
        .route("/v2/", any(handle::<T, C>))
        .route("/v2/{*rest}", any(handle::<T, C>))
        .with_state(Arc::new(registry))
}

/// What the proxy does with a request for the chart's repository.
#[derive(Debug, PartialEq)]
enum ChartRequest<'a> {
    /// List only the install's version.
    Tags,
    /// A version by tag: served only if it's the install's.
    Version(&'a str),
    /// By digest, or a signature's or attestation's tag (`sha256-…`): passed through, since a
    /// digest only names what a version the install got already points to.
    Other,
}

fn chart_request(path: &str) -> ChartRequest<'_> {
    let rest = path.strip_prefix("/v2/").and_then(|p| p.strip_prefix(CHART_REPO)).unwrap_or_default();
    if rest == "/tags/list" {
        return ChartRequest::Tags;
    }
    match rest.strip_prefix("/manifests/") {
        Some(reference) if !reference.starts_with("sha256:") && !reference.starts_with("sha256-") => {
            ChartRequest::Version(reference)
        }
        _ => ChartRequest::Other,
    }
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
    // The path goes upstream as it came, so nothing in it may climb out of the repository:
    // no dot segments, and no percent-encoding to hide one (or a slash) in.
    if rest.contains('%') || rest.split('/').any(|part| part == "." || part == "..") {
        return None;
    }
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

async fn handle<T: Tokens, C: Channels>(State(reg): State<Arc<Registry<T, C>>>, req: Request) -> Response {
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
    if target == (Target::Repo { name: CHART_REPO }) {
        let request = chart_request(path);
        if request != ChartRequest::Other {
            let allowed = match reg.channels.chart_version(install).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(error = %e, "finding an install's chart version failed");
                    return error(StatusCode::SERVICE_UNAVAILABLE, "UNAVAILABLE", "try again");
                }
            };
            match request {
                ChartRequest::Tags => {
                    let body =
                        serde_json::json!({ "name": CHART_REPO, "tags": allowed.into_iter().collect::<Vec<_>>() });
                    return (StatusCode::OK, [(header::CONTENT_TYPE, "application/json")], body.to_string())
                        .into_response();
                }
                ChartRequest::Version(v) if allowed.as_deref() != Some(v) => {
                    tracing::info!(%install, version = v, allowed = ?allowed, "refused a chart version outside the install's channel");
                    return error(
                        StatusCode::NOT_FOUND,
                        "MANIFEST_UNKNOWN",
                        "your release channel names another version",
                    );
                }
                _ => {}
            }
        }
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
        assert_eq!(parse("/v2/awzy/x/manifests/../../../other/manifests/y"), None);
        assert_eq!(parse("/v2/awzy/x/blobs/..%2F..%2Fother/manifests/y"), None);
        assert_eq!(parse("/v2/awzy/x/manifests/%2e%2e"), None);
        assert_eq!(parse("/v2/awzy/x/blobs/uploads/"), Some(Target::Repo { name: "awzy/x" }), "rejected by method");
        assert_eq!(parse("/v2/_catalog"), None);
    }

    struct OneToken(InstallId);
    impl Tokens for OneToken {
        async fn check(&self, install: InstallId, token: &str) -> anyhow::Result<bool> {
            Ok(install == self.0 && token == "awzy_pull_good")
        }
    }

    /// Every install's channel names one version.
    struct OneVersion(&'static str);
    impl Channels for OneVersion {
        async fn chart_version(&self, _: InstallId) -> anyhow::Result<Option<String>> {
            Ok(Some(self.0.to_string()))
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
            .route("/v2/awzy/charts/awzy/manifests/{reference}", get(|| async { "chart" }))
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
        let registry = Registry {
            tokens: OneToken(install),
            channels: OneVersion("0.1.0-g000000000002"),
            upstream,
            realm: "registry.test".into(),
        };
        (router(registry), install)
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

    #[tokio::test]
    async fn an_install_sees_and_gets_only_its_channels_chart_version() {
        let (app, install) = app().await;
        let user = install.to_string();
        let get = |path: &str| app.clone().oneshot(get_req(path, &user, "awzy_pull_good"));

        let resp = get("/v2/awzy/charts/awzy/tags/list").await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(body["tags"], serde_json::json!(["0.1.0-g000000000002"]));

        assert_eq!(get("/v2/awzy/charts/awzy/manifests/0.1.0-g000000000002").await.unwrap().status(), StatusCode::OK);
        assert_eq!(
            get("/v2/awzy/charts/awzy/manifests/0.1.0-g000000000001").await.unwrap().status(),
            StatusCode::NOT_FOUND
        );
        // By digest, and the signatures beside a version, pass through.
        assert_eq!(get("/v2/awzy/charts/awzy/manifests/sha256:cc").await.unwrap().status(), StatusCode::OK);
        assert_eq!(get("/v2/awzy/charts/awzy/manifests/sha256-cc.sig").await.unwrap().status(), StatusCode::OK);
    }

    #[test]
    fn chart_requests_are_told_apart() {
        assert_eq!(chart_request("/v2/awzy/charts/awzy/tags/list"), ChartRequest::Tags);
        assert_eq!(chart_request("/v2/awzy/charts/awzy/manifests/0.1.0-gabc"), ChartRequest::Version("0.1.0-gabc"));
        assert_eq!(chart_request("/v2/awzy/charts/awzy/manifests/sha256:00"), ChartRequest::Other);
        assert_eq!(chart_request("/v2/awzy/charts/awzy/blobs/sha256:00"), ChartRequest::Other);
    }
}

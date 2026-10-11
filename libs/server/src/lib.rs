//! HTTP server runtime shared by every service: health routes, metrics, listen address, graceful
//! shutdown.

pub mod metrics;

use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{http::StatusCode, routing::get, Router};
use tokio::net::TcpListener;

/// Whether the service can take traffic. `/readyz` answers 503 until it returns true.
pub type Readiness = Arc<dyn Fn() -> bool + Send + Sync>;

/// What `/version` reports: the release, and the image digest when AWZY_IMAGE_DIGEST is set.
/// awzy-reporter reads it (ADR 0026).
///
/// The release comes from AWZY_RELEASE_VERSION at runtime, which deployments set to the commit
/// they deployed (the chart from its appVersion, the corp portal's deploy script from its tag).
/// Builds don't stamp it: a version compiled in would make every commit rebuild every crate.
pub fn version() -> serde_json::Value {
    version_from(std::env::var("AWZY_RELEASE_VERSION").ok(), std::env::var("AWZY_IMAGE_DIGEST").ok())
}

fn version_from(release: Option<String>, image_digest: Option<String>) -> serde_json::Value {
    let release = release.filter(|r| !r.is_empty());
    serde_json::json!({
        "version": release.as_deref().unwrap_or(env!("CARGO_PKG_VERSION")),
        "image_digest": image_digest,
    })
}

/// Adds the Kubernetes liveness and readiness routes, always ready.
pub fn with_health(app: Router) -> Router {
    with_readiness(app, Arc::new(|| true))
}

/// Adds the Kubernetes liveness route and a readiness route that follows `ready`.
pub fn with_readiness(app: Router, ready: Readiness) -> Router {
    app.route("/healthz", get(|| async { "ok" })).route("/version", get(|| async { axum::Json(version()) })).route(
        "/readyz",
        get(move || {
            let ready = ready.clone();
            async move {
                if ready() {
                    (StatusCode::OK, "ok")
                } else {
                    (StatusCode::SERVICE_UNAVAILABLE, "not ready")
                }
            }
        }),
    )
}

/// Serves `app` on `AWZY_LISTEN_ADDR` (default `0.0.0.0:8080`) until SIGTERM or Ctrl-C, with the
/// health routes and `/metrics` (ADR 0068).
pub async fn serve(service: &'static str, app: Router) -> anyhow::Result<()> {
    serve_with_readiness(service, app, Arc::new(|| true)).await
}

/// Like [`serve`], with `/readyz` following `ready`.
///
/// On SIGTERM it drains (ADR 0072): `/readyz` fails at once, it keeps serving for
/// `AWZY_SHUTDOWN_DELAY_SECS` (default 0) while Kubernetes stops routing to it, then it finishes
/// in-flight requests and closes HTTP/2 and gRPC connections with GOAWAY.
pub async fn serve_with_readiness(service: &'static str, app: Router, ready: Readiness) -> anyhow::Result<()> {
    let addr: SocketAddr = std::env::var("AWZY_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()).parse()?;
    let delay = std::env::var("AWZY_SHUTDOWN_DELAY_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(service, %addr, "listening");
    let stopping = Arc::new(AtomicBool::new(false));
    let ready = draining(ready, stopping.clone());
    metrics::register(Arc::new(metrics::Readiness(ready.clone())));
    let app = metrics::with_metrics(with_readiness(app, ready));
    let drained = async move {
        shutdown_signal().await;
        stopping.store(true, Ordering::Release);
        if delay > 0 {
            tracing::info!(delay_secs = delay, "not ready; serving until traffic moves elsewhere");
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
        tracing::info!("finishing in-flight requests");
    };
    axum::serve(listener, app).with_graceful_shutdown(drained).await?;
    Ok(())
}

/// `ready`, until shutdown begins.
fn draining(ready: Readiness, stopping: Arc<AtomicBool>) -> Readiness {
    Arc::new(move || !stopping.load(Ordering::Acquire) && ready())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn readyz_follows_readiness() {
        let flag = Arc::new(AtomicBool::new(false));
        let f = flag.clone();
        let app = with_readiness(Router::new(), Arc::new(move || f.load(Ordering::Acquire)));
        let get = |app: Router| async move {
            app.oneshot(Request::get("/readyz").body(Body::empty()).unwrap()).await.unwrap().status()
        };
        assert_eq!(get(app.clone()).await, 503);
        flag.store(true, Ordering::Release);
        assert_eq!(get(app).await, 200);
    }

    #[tokio::test]
    async fn a_draining_server_is_not_ready_but_still_serves() {
        let stopping = Arc::new(AtomicBool::new(false));
        let ready = draining(Arc::new(|| true), stopping.clone());
        assert!(ready());
        stopping.store(true, Ordering::Release);
        assert!(!ready(), "Kubernetes stops routing to it");
        let app = with_readiness(Router::new().route("/work", get(|| async { "done" })), ready);
        let status = |path: &'static str| {
            let app = app.clone();
            async move { app.oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap().status() }
        };
        assert_eq!(status("/readyz").await, 503);
        assert_eq!(status("/work").await, 200, "requests already routed to it still succeed");
    }

    #[tokio::test]
    async fn health_routes_respond() {
        for path in ["/healthz", "/readyz"] {
            let res =
                with_health(Router::new()).oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(res.status(), 200, "{path}");
        }
    }

    #[test]
    fn the_deployed_release_wins_over_the_crate_version() {
        let v = version_from(Some("13789b7a".into()), Some("sha256:abc".into()));
        assert_eq!((v["version"].as_str(), v["image_digest"].as_str()), (Some("13789b7a"), Some("sha256:abc")));
        assert_eq!(
            version_from(None, None)["version"],
            env!("CARGO_PKG_VERSION"),
            "a local run reports the crate version"
        );
        assert_eq!(version_from(Some(String::new()), None)["version"], env!("CARGO_PKG_VERSION"));
    }
}

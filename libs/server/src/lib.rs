//! HTTP server runtime shared by every service: health routes, listen address, graceful shutdown.

use std::{net::SocketAddr, sync::Arc};

use axum::{http::StatusCode, routing::get, Router};
use tokio::net::TcpListener;

/// Whether the service can take traffic. `/readyz` answers 503 until it returns true.
pub type Readiness = Arc<dyn Fn() -> bool + Send + Sync>;

/// What `/version` reports: the release, and the image digest when the chart sets
/// AWZY_IMAGE_DIGEST. awzy-reporter reads it (ADR 0026).
fn version() -> serde_json::Value {
    serde_json::json!({
        "version": option_env!("AWZY_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
        "image_digest": std::env::var("AWZY_IMAGE_DIGEST").ok(),
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

/// Serves `app` on `AWZY_LISTEN_ADDR` (default `0.0.0.0:8080`) until SIGTERM or Ctrl-C.
pub async fn serve(service: &'static str, app: Router) -> anyhow::Result<()> {
    serve_with_readiness(service, app, Arc::new(|| true)).await
}

/// Like [`serve`], with `/readyz` following `ready`.
pub async fn serve_with_readiness(service: &'static str, app: Router, ready: Readiness) -> anyhow::Result<()> {
    let addr: SocketAddr = std::env::var("AWZY_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()).parse()?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(service, %addr, "listening");
    axum::serve(listener, with_readiness(app, ready)).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
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
        use std::sync::atomic::{AtomicBool, Ordering};
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
    async fn health_routes_respond() {
        for path in ["/healthz", "/readyz"] {
            let res =
                with_health(Router::new()).oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(res.status(), 200, "{path}");
        }
    }
}

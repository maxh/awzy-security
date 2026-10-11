//! The Kubernetes API from inside a pod, as the pod's service account (#223): awzy-reporter
//! reading workloads, the client portal starting repairs. Plain HTTPS calls rather than a full
//! Kubernetes client library: awzy needs a few list, get, and create calls, and what each may do is
//! the chart's RBAC.

use std::time::Duration;

use awzy_http::{encode, Body, Client, HttpError, Method, Request};
use rustls_pki_types::{pem::PemObject, CertificateDer};
use serde_json::Value;

const SA_DIR: &str = "/var/run/secrets/kubernetes.io/serviceaccount";
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum KubeError {
    #[error("not running in Kubernetes (KUBERNETES_SERVICE_HOST is unset)")]
    NotInCluster,
    #[error("reading {path}: {source}")]
    ServiceAccount {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("the cluster CA: {0}")]
    Ca(String),
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error("{method} {path}: the API server answered {status}: {body}")]
    Refused { method: Method, path: String, status: u16, body: String },
    #[error("{method} {path}: the answer isn't JSON: {source}")]
    Json {
        method: Method,
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

enum Token {
    /// Re-read on every call: projected service account tokens rotate.
    File(String),
    /// For tests against a fake API server.
    Fixed(String),
}

pub struct KubeApi {
    base: String,
    token: Token,
    client: Client,
}

impl KubeApi {
    /// From inside a pod: the API server from KUBERNETES_SERVICE_HOST, trusting the cluster CA.
    pub fn in_cluster() -> Result<Self, KubeError> {
        let host = std::env::var("KUBERNETES_SERVICE_HOST").map_err(|_| KubeError::NotInCluster)?;
        let port = std::env::var("KUBERNETES_SERVICE_PORT").unwrap_or_else(|_| "443".into());
        let host = if host.contains(':') { format!("[{host}]") } else { host };
        let ca_path = format!("{SA_DIR}/ca.crt");
        let ca = std::fs::read(&ca_path).map_err(|source| KubeError::ServiceAccount { path: ca_path, source })?;
        let mut roots = rustls::RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(&ca) {
            roots.add(cert.map_err(|e| KubeError::Ca(e.to_string()))?).map_err(|e| KubeError::Ca(e.to_string()))?;
        }
        Ok(Self {
            base: format!("https://{host}:{port}"),
            token: Token::File(format!("{SA_DIR}/token")),
            client: awzy_http::client_with_roots(roots),
        })
    }

    /// Against a fake API server, over plain HTTP.
    pub fn for_test(base: impl Into<String>) -> Self {
        Self { base: base.into(), token: Token::Fixed("test-token".into()), client: awzy_http::client() }
    }

    async fn call(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value, KubeError> {
        let token = match &self.token {
            Token::File(p) => {
                std::fs::read_to_string(p).map_err(|source| KubeError::ServiceAccount { path: p.clone(), source })?
            }
            Token::Fixed(t) => t.clone(),
        };
        let req = Request::builder()
            .method(method.clone())
            .uri(format!("{}{path}", self.base))
            .header("authorization", format!("Bearer {}", token.trim()))
            .header("content-type", "application/json")
            .body(body.map(|b| Body::from(b.to_string())).unwrap_or_default())
            .map_err(HttpError::from)?;
        let res = awzy_http::send(&self.client, req, TIMEOUT).await?;
        let path = path.split('?').next().unwrap_or(path).to_string();
        if !res.status.is_success() {
            return Err(KubeError::Refused { method, path, status: res.status.as_u16(), body: res.text() });
        }
        serde_json::from_slice(&res.body).map_err(|source| KubeError::Json { method, path, source })
    }

    /// The items at `path` whose labels match `selector` (e.g. `app.kubernetes.io/part-of=awzy`).
    pub async fn list(&self, path: &str, selector: &str) -> Result<Vec<Value>, KubeError> {
        let list = self.call(Method::GET, &format!("{path}?labelSelector={}", encode(selector)), None).await?;
        Ok(list["items"].as_array().cloned().unwrap_or_default())
    }

    /// One object.
    pub async fn get(&self, path: &str) -> Result<Value, KubeError> {
        self.call(Method::GET, path, None).await
    }

    /// Creates `object` at `path` and returns it as the API server made it.
    pub async fn create(&self, path: &str, object: &Value) -> Result<Value, KubeError> {
        self.call(Method::POST, path, Some(object)).await
    }
}

#[cfg(test)]
mod tests {
    use axum::{extract::Query, http::HeaderMap, routing::get, Json, Router};
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn lists_by_label_and_says_why_a_call_was_refused() {
        let app = Router::new()
            .route(
                "/api/v1/namespaces/awzy/pods",
                get(|headers: HeaderMap, Query(q): Query<std::collections::HashMap<String, String>>| async move {
                    assert_eq!(headers["authorization"], "Bearer test-token");
                    assert_eq!(q["labelSelector"], "app.kubernetes.io/part-of=awzy");
                    Json(json!({ "items": [{ "metadata": { "name": "awzy-pdp-0" } }] }))
                }),
            )
            .route("/api/v1/namespaces/awzy/secrets", get(|| async { (axum::http::StatusCode::FORBIDDEN, "no") }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let kube = KubeApi::for_test(base);

        let pods = kube.list("/api/v1/namespaces/awzy/pods", "app.kubernetes.io/part-of=awzy").await.unwrap();
        assert_eq!(pods[0]["metadata"]["name"], "awzy-pdp-0");
        let err = kube.get("/api/v1/namespaces/awzy/secrets").await.unwrap_err();
        assert!(matches!(err, KubeError::Refused { status: 403, .. }), "{err}");
    }
}

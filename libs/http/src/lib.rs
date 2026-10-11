//! One way for awzy's services to make HTTP calls (#223): a client that speaks plain HTTP and
//! HTTPS (rustls on ring, the web PKI's roots, or a given CA), a timeout that covers the whole
//! response including its body, and percent-encoding. Services build on this rather than on
//! hyper directly, so timeouts and TLS settings are the same everywhere.

use std::time::Duration;

use bytes::Bytes;
pub use http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client as Hyper},
    rt::TokioExecutor,
};

/// A request body: empty, or bytes.
pub type Body = Full<Bytes>;

/// A pooled HTTP and HTTPS client. Cheap to clone.
pub type Client = Hyper<hyper_rustls::HttpsConnector<HttpConnector>, Body>;

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("{method} {url} timed out after {after:?}")]
    Timeout { method: Method, url: String, after: Duration },
    #[error("{method} {url}: {source}")]
    Transport {
        method: Method,
        url: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("building the request: {0}")]
    Request(#[from] http::Error),
}

/// A whole response: status, headers, and the body read to the end.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl Response {
    /// The body as text, for error messages.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// HTTPS with the web PKI's roots, and plain HTTP: for awzy's own services in the cluster and
/// the outside services they call.
pub fn client() -> Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let https = hyper_rustls::HttpsConnectorBuilder::new().with_webpki_roots().https_or_http().enable_http1().build();
    Hyper::builder(TokioExecutor::new()).build(https)
}

/// HTTPS trusting only `roots`, such as the cluster's CA, and nothing over plain HTTP.
pub fn client_with_roots(roots: rustls::RootCertStore) -> Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let tls = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    let https = hyper_rustls::HttpsConnectorBuilder::new().with_tls_config(tls).https_only().enable_http1().build();
    Hyper::builder(TokioExecutor::new()).build(https)
}

/// Sends `req` and reads the whole response, giving up after `timeout`: a server that answers
/// and then stalls mid-body can't hold the caller.
pub async fn send(client: &Client, req: Request<Body>, timeout: Duration) -> Result<Response, HttpError> {
    let (method, url) = (req.method().clone(), without_query(&req.uri().to_string()).to_string());
    let call = async {
        let res = client.request(req).await.map_err(|e| HttpError::Transport {
            method: method.clone(),
            url: url.clone(),
            source: Box::new(e),
        })?;
        let (parts, body) = res.into_parts();
        let body = body.collect().await.map_err(|e| HttpError::Transport {
            method: method.clone(),
            url: url.clone(),
            source: Box::new(e),
        })?;
        Ok(Response { status: parts.status, headers: parts.headers, body: body.to_bytes() })
    };
    tokio::time::timeout(timeout, call).await.map_err(|_| HttpError::Timeout { method, url, after: timeout })?
}

/// A GET of `url` with `headers`, through [`send`].
pub async fn get(
    client: &Client,
    url: &str,
    headers: &[(&str, &str)],
    timeout: Duration,
) -> Result<Response, HttpError> {
    let mut req = Request::get(url);
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    send(client, req.body(Body::default())?, timeout).await
}

/// For errors and logs: query strings carry page tokens and such, and none of them help.
fn without_query(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

/// Percent-encodes a path segment or query value: everything but RFC 3986's unreserved
/// characters.
pub fn encode(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for b in v.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use axum::{routing::get as route_get, Router};

    use super::*;

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    #[test]
    fn encodes_everything_but_unreserved_characters() {
        assert_eq!(encode("synthetic:doc/1 a~b"), "synthetic%3Adoc%2F1%20a~b");
    }

    #[tokio::test]
    async fn a_response_is_read_whole_and_a_stall_times_out() {
        let app = Router::new().route("/ok", route_get(|| async { "fine" })).route(
            "/slow",
            route_get(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                "late"
            }),
        );
        let base = serve(app).await;
        let c = client();
        let ok = get(&c, &format!("{base}/ok?token=secret"), &[], Duration::from_secs(5)).await.unwrap();
        assert_eq!((ok.status, ok.text().as_str()), (StatusCode::OK, "fine"));
        let err = get(&c, &format!("{base}/slow?token=secret"), &[], Duration::from_millis(100)).await.unwrap_err();
        assert!(matches!(err, HttpError::Timeout { .. }), "{err}");
        assert!(!err.to_string().contains("secret"), "no query strings in errors: {err}");
    }
}

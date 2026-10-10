//! A small JSON-over-HTTPS client: the connector's requests to Google and to awzy-context.

use std::time::Duration;

use anyhow::Context;
use axum::http::{header, HeaderValue, Method, Request, StatusCode};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct Http {
    client: Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>,
}

impl Default for Http {
    fn default() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let https =
            hyper_rustls::HttpsConnectorBuilder::new().with_webpki_roots().https_or_http().enable_http1().build();
        Self { client: Client::builder(TokioExecutor::new()).build(https) }
    }
}

/// A request body: none, JSON, or a form.
pub enum Body<'a> {
    None,
    Json(&'a Value),
    Form(String),
}

impl Http {
    /// Sends one request with `headers` and returns the status and the JSON body (`Null` when
    /// empty or not JSON).
    pub async fn send(
        &self,
        method: Method,
        url: &str,
        headers: &[(&str, String)],
        body: Body<'_>,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let mut req = Request::builder().method(method.clone()).uri(url);
        for (name, value) in headers {
            req = req.header(*name, HeaderValue::from_str(value)?);
        }
        let bytes = match body {
            Body::None => Bytes::new(),
            Body::Json(v) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                Bytes::from(serde_json::to_vec(v)?)
            }
            Body::Form(f) => {
                req = req.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
                Bytes::from(f)
            }
        };
        let res = tokio::time::timeout(TIMEOUT, self.client.request(req.body(Full::new(bytes))?))
            .await
            .with_context(|| format!("{method} {} timed out", without_query(url)))?
            .with_context(|| format!("{method} {}", without_query(url)))?;
        let status = res.status();
        let body = res.into_body().collect().await?.to_bytes();
        Ok((status, serde_json::from_slice(&body).unwrap_or(Value::Null)))
    }
}

/// For error messages: query strings carry page tokens, and none of them help.
fn without_query(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

/// Percent-encodes a query parameter value.
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
    use super::*;

    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(encode("a b/c=d&e"), "a%20b%2Fc%3Dd%26e");
        assert_eq!(encode("trashed = false"), "trashed%20%3D%20false");
    }
}

//! A small JSON-over-HTTPS client on `awzy_http`: the connectors' requests to Google, Jira, and
//! awzy-context.

use std::time::Duration;

use anyhow::Context;
pub use awzy_http::encode;
use awzy_http::{header, HeaderValue, Method, Request, StatusCode};
use bytes::Bytes;
use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct Http {
    client: awzy_http::Client,
}

impl Default for Http {
    fn default() -> Self {
        Self { client: awzy_http::client() }
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
    /// empty or not JSON). The timeout covers the whole response, body included.
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
        let res = awzy_http::send(&self.client, req.body(awzy_http::Body::new(bytes))?, TIMEOUT)
            .await
            .with_context(|| format!("{method} {}", url.split('?').next().unwrap_or(url)))?;
        Ok((res.status, serde_json::from_slice(&res.body).unwrap_or(Value::Null)))
    }
}

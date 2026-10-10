//! The registry the proxy reads from: ECR in awzy-build. Requests go out with ECR's Basic
//! credentials and come back unchanged, including ECR's redirects of layer downloads to
//! presigned S3 URLs, which the client follows itself.

use std::{sync::Arc, time::Duration};

use anyhow::{bail, Context};
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, Response};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::sigv4;

pub type HttpClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>;

/// HTTP and HTTPS (rustls with the webpki roots).
pub fn http_client() -> HttpClient {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let https = hyper_rustls::HttpsConnectorBuilder::new().with_webpki_roots().https_or_http().enable_http1().build();
    Client::builder(TokioExecutor::new()).build(https)
}

/// Headers passed back to the client from the upstream registry.
const PASSED_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "docker-content-digest",
    "docker-distribution-api-version",
    "etag",
    "location",
    "link",
    "oci-filters-applied",
];

/// How the proxy authenticates to the upstream registry.
pub enum Auth {
    /// ECR: a 12-hour token from GetAuthorizationToken, refreshed before it expires.
    Ecr(Box<EcrAuth>),
    /// A fixed `Authorization` value (tests and local registries).
    Fixed(Option<String>),
}

pub struct Upstream {
    /// e.g. `https://123456789012.dkr.ecr.us-east-2.amazonaws.com`
    pub base_url: String,
    pub client: HttpClient,
    pub auth: Auth,
}

impl Upstream {
    /// ECR at `host`, e.g. `123456789012.dkr.ecr.us-east-2.amazonaws.com`.
    pub fn ecr(host: &str) -> anyhow::Result<Self> {
        let region =
            host.split('.').nth(3).filter(|_| host.contains(".dkr.ecr.")).context("not an ECR registry host")?;
        let client = http_client();
        Ok(Self {
            base_url: format!("https://{host}"),
            client: client.clone(),
            auth: Auth::Ecr(Box::new(EcrAuth::new(region.to_string(), client))),
        })
    }

    /// Forwards a GET or HEAD for `path` (`/v2/...` plus query) and returns the upstream response.
    pub async fn forward(
        &self,
        method: Method,
        path_and_query: &str,
        accept: Option<&HeaderValue>,
    ) -> anyhow::Result<Response<axum::body::Body>> {
        let mut req = Request::builder().method(method).uri(format!("{}{path_and_query}", self.base_url));
        if let Some(accept) = accept {
            req = req.header(header::ACCEPT, accept);
        }
        let authorization = match &self.auth {
            Auth::Ecr(ecr) => Some(ecr.basic().await?),
            Auth::Fixed(v) => v.clone(),
        };
        if let Some(v) = authorization {
            req = req.header(header::AUTHORIZATION, v);
        }
        let resp = tokio::time::timeout(Duration::from_secs(30), self.client.request(req.body(Full::default())?))
            .await
            .context("upstream registry timed out")??;
        let mut out = Response::builder().status(resp.status());
        let headers = out.headers_mut().expect("builder is valid");
        copy_headers(resp.headers(), headers);
        Ok(out.body(axum::body::Body::new(resp.into_body()))?)
    }
}

fn copy_headers(from: &HeaderMap, to: &mut HeaderMap) {
    for name in PASSED_HEADERS {
        for v in from.get_all(*name) {
            to.append(*name, v.clone());
        }
    }
}

/// ECR credentials for the registry API, from the task's AWS credentials.
pub struct EcrAuth {
    region: String,
    client: HttpClient,
    cached: Mutex<Option<(String, DateTime<Utc>)>>,
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(rename = "authorizationData")]
    data: Vec<TokenData>,
}

#[derive(Deserialize)]
struct TokenData {
    #[serde(rename = "authorizationToken")]
    token: String,
    #[serde(rename = "expiresAt")]
    expires_at: f64,
}

impl EcrAuth {
    pub fn new(region: String, client: HttpClient) -> Self {
        Self { region, client, cached: Mutex::new(None) }
    }

    /// `Basic <token>`, refreshed when less than an hour is left.
    pub async fn basic(&self) -> anyhow::Result<String> {
        let mut cached = self.cached.lock().await;
        if let Some((token, expires)) = cached.as_ref() {
            if *expires - Utc::now() > chrono::Duration::hours(1) {
                return Ok(format!("Basic {token}"));
            }
        }
        let (token, expires) = self.fetch().await?;
        tracing::info!(%expires, "fetched an ECR authorization token");
        *cached = Some((token.clone(), expires));
        Ok(format!("Basic {token}"))
    }

    async fn fetch(&self) -> anyhow::Result<(String, DateTime<Utc>)> {
        let creds = credentials(&self.client).await?;
        let host = format!("api.ecr.{}.amazonaws.com", self.region);
        let body = b"{}";
        let headers = [
            ("content-type", "application/x-amz-json-1.1"),
            ("x-amz-target", "AmazonEC2ContainerRegistry_V20150921.GetAuthorizationToken"),
        ];
        let amz_date = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let signed = sigv4::sign(
            &creds,
            &self.region,
            "ecr",
            &sigv4::Request { method: "POST", host: &host, path: "/", query: "", headers: &headers, payload: body },
            &amz_date,
        );
        let mut req = Request::builder().method(Method::POST).uri(format!("https://{host}/"));
        for (k, v) in headers.iter() {
            req = req.header(*k, *v);
        }
        for (k, v) in signed {
            req = req.header(k, v);
        }
        let resp = self.client.request(req.body(Full::new(Bytes::from_static(body)))?).await?;
        let status = resp.status();
        let bytes = resp.into_body().collect().await?.to_bytes();
        if !status.is_success() {
            bail!("ECR GetAuthorizationToken: {status}: {}", String::from_utf8_lossy(&bytes));
        }
        let parsed: TokenResponse = serde_json::from_slice(&bytes)?;
        let data = parsed.data.into_iter().next().context("ECR returned no authorization data")?;
        let expires = DateTime::from_timestamp(data.expires_at as i64, 0).context("bad expiresAt")?;
        Ok((data.token, expires))
    }
}

#[derive(Deserialize)]
struct ContainerCredentials {
    #[serde(rename = "AccessKeyId")]
    access_key_id: String,
    #[serde(rename = "SecretAccessKey")]
    secret_access_key: String,
    #[serde(rename = "Token")]
    token: Option<String>,
}

/// AWS credentials: AWS_ACCESS_KEY_ID and friends (local runs), or the ECS task role
/// (AWS_CONTAINER_CREDENTIALS_RELATIVE_URI, set by ECS).
async fn credentials(client: &HttpClient) -> anyhow::Result<sigv4::Credentials> {
    if let (Ok(id), Ok(secret)) = (std::env::var("AWS_ACCESS_KEY_ID"), std::env::var("AWS_SECRET_ACCESS_KEY")) {
        return Ok(sigv4::Credentials {
            access_key_id: id,
            secret_access_key: secret,
            session_token: std::env::var("AWS_SESSION_TOKEN").ok(),
        });
    }
    let uri = std::env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI")
        .map(|p| format!("http://169.254.170.2{p}"))
        .context("no AWS credentials: set AWS_ACCESS_KEY_ID or run with an ECS task role")?;
    let resp = client.request(Request::get(uri).body(Full::default())?).await?;
    let bytes = resp.into_body().collect().await?.to_bytes();
    let c: ContainerCredentials = serde_json::from_slice(&bytes).context("parsing ECS task credentials")?;
    Ok(sigv4::Credentials {
        access_key_id: c.access_key_id,
        secret_access_key: c.secret_access_key,
        session_token: c.token,
    })
}

/// Shared by every request.
pub type SharedUpstream = Arc<Upstream>;

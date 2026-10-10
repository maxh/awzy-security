//! Signing in to Google as a service account with domain-wide delegation (ADR 0069): a JWT signed
//! with the account's key, exchanged for an access token that acts as `subject`.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use anyhow::{bail, Context};
use axum::http::Method;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

use crate::http::{encode, Body, Http};

/// Read-only Drive metadata: names, folders, sharing. No content.
pub const DRIVE_METADATA: &str = "https://www.googleapis.com/auth/drive.metadata.readonly";
/// The domain's groups, to name group shares as SCIM does.
pub const DIRECTORY_GROUPS: &str = "https://www.googleapis.com/auth/admin.directory.group.readonly";

/// A service account key file, as Google Cloud issues it.
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceAccountKey {
    pub client_email: String,
    pub private_key: String,
    #[serde(default = "token_uri")]
    pub token_uri: String,
}

fn token_uri() -> String {
    "https://oauth2.googleapis.com/token".into()
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    sub: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
}

pub struct Auth {
    key: ServiceAccountKey,
    signing: EncodingKey,
    http: Http,
    /// (subject, scope) to an access token and when to stop using it.
    tokens: Mutex<HashMap<(String, String), (String, Instant)>>,
}

impl Auth {
    pub fn new(key: ServiceAccountKey, http: Http) -> anyhow::Result<Self> {
        let signing =
            EncodingKey::from_rsa_pem(key.private_key.as_bytes()).context("the service account's private key")?;
        Ok(Self { key, signing, http, tokens: Mutex::default() })
    }

    pub fn from_file(path: &str, http: Http) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        Self::new(serde_json::from_str(&text).with_context(|| format!("{path} isn't a service account key"))?, http)
    }

    /// An access token acting as `subject` for `scope`, reused until a minute before it expires.
    pub async fn token(&self, subject: &str, scope: &str) -> anyhow::Result<String> {
        let cache_key = (subject.to_string(), scope.to_string());
        if let Some((token, until)) = self.tokens.lock().unwrap_or_else(|e| e.into_inner()).get(&cache_key) {
            if Instant::now() < *until {
                return Ok(token.clone());
            }
        }
        let now = chrono::Utc::now().timestamp();
        let claims = Claims {
            iss: &self.key.client_email,
            sub: subject,
            scope,
            aud: &self.key.token_uri,
            iat: now,
            exp: now + 3600,
        };
        let assertion = jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &self.signing)?;
        let form = format!(
            "grant_type={}&assertion={}",
            encode("urn:ietf:params:oauth:grant-type:jwt-bearer"),
            encode(&assertion)
        );
        let (status, body) = self.http.send(Method::POST, &self.key.token_uri, &[], Body::Form(form)).await?;
        if !status.is_success() {
            // Google's error names the problem (unauthorized_client: delegation isn't granted).
            bail!("Google refused a token for {subject} ({status}): {body}");
        }
        let token = body["access_token"].as_str().context("no access_token from Google")?.to_string();
        let lifetime =
            Duration::from_secs(body["expires_in"].as_u64().unwrap_or(3600)).saturating_sub(Duration::from_secs(60));
        self.tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(cache_key, (token.clone(), Instant::now() + lifetime));
        Ok(token)
    }
}

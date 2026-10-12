//! Claude on Google Cloud's Vertex AI, in the install's own project (ADR 0114).
//!
//! Requests go to Vertex AI's `rawPredict` for Anthropic's models, with Claude's Messages API
//! body. The access token comes from the GKE metadata server as the service's own Google service
//! account (Workload Identity), so there's no key to manage. The client enables Claude in Vertex
//! AI's Model Garden for the project; until then requests fail with [`LlmError::NotConfigured`].

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use hyper::{header, Request, StatusCode};
use serde::Deserialize;

use crate::{
    messages::{parse_response, request_body},
    LlmError, ModelProvider, ModelRequest, ModelResponse, ModelSettings,
};

/// Vertex AI's model ID for Claude Opus 5.5. A pinned version is named with `@`, e.g.
/// `claude-opus-5-5@YYYYMMDD`; set AWZY_LLM_MODEL.
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";

/// The location that routes to any region with capacity. A region (`us-east5`) keeps requests
/// in it.
pub const DEFAULT_REGION: &str = "global";

/// Vertex AI's version tag for Claude's Messages API body.
const ANTHROPIC_VERSION: &str = "vertex-2023-10-16";

/// A model's answer can take a while; the token request shouldn't.
const RESPOND_TIMEOUT: Duration = Duration::from_secs(300);
const TOKEN_TIMEOUT: Duration = Duration::from_secs(5);

/// The service's own access token, from the metadata server.
const TOKEN_URL: &str = "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

/// Where the model is, besides [`ModelSettings`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexLocation {
    pub project: String,
    /// A region, or `global`.
    pub region: String,
}

pub struct Vertex {
    client: awzy_http::Client,
    config: ModelSettings,
    location: VertexLocation,
    /// A token from somewhere other than the metadata server (tests, a laptop's gcloud).
    fixed_token: Option<String>,
    token: Mutex<Option<(String, Instant)>>,
}

#[derive(Deserialize)]
struct AccessToken {
    access_token: String,
    expires_in: u64,
}

impl Vertex {
    /// Tokens from the metadata server.
    pub fn new(config: ModelSettings, location: VertexLocation) -> Self {
        Self { client: awzy_http::client(), config, location, fixed_token: None, token: Mutex::new(None) }
    }

    /// With this access token instead, e.g. `gcloud auth print-access-token`'s.
    pub fn with_token(mut self, token: String) -> Self {
        self.fixed_token = Some(token);
        self
    }

    /// The model's ID.
    pub fn model(&self) -> &str {
        &self.config.model
    }

    fn url(&self) -> String {
        let VertexLocation { project, region } = &self.location;
        let host = if region == "global" {
            "aiplatform.googleapis.com".to_string()
        } else {
            format!("{region}-aiplatform.googleapis.com")
        };
        format!(
            "https://{host}/v1/projects/{project}/locations/{region}/publishers/anthropic/models/{}:rawPredict",
            self.config.model
        )
    }

    /// A token that's good for at least another minute.
    async fn token(&self) -> Result<String, LlmError> {
        if let Some(token) = &self.fixed_token {
            return Ok(token.clone());
        }
        if let Some((token, until)) = self.token.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            if Instant::now() + Duration::from_secs(60) < until {
                return Ok(token);
            }
        }
        let res = awzy_http::get(&self.client, TOKEN_URL, &[("Metadata-Flavor", "Google")], TOKEN_TIMEOUT)
            .await
            .map_err(|e| LlmError::NotConfigured(format!("no access token from the metadata server: {e}")))?;
        if !res.status.is_success() {
            return Err(LlmError::NotConfigured(format!("the metadata server answered {}", res.status)));
        }
        let t: AccessToken = serde_json::from_slice(&res.body).map_err(|e| LlmError::Malformed(e.to_string()))?;
        let until = Instant::now() + Duration::from_secs(t.expires_in);
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) = Some((t.access_token.clone(), until));
        Ok(t.access_token)
    }
}

impl ModelProvider for Vertex {
    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, LlmError> {
        let body = request_body(request, ANTHROPIC_VERSION, self.config.effort.as_deref());
        let body = serde_json::to_vec(&body).map_err(|e| LlmError::Failed(e.to_string()))?;
        let req = Request::post(self.url())
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token().await?))
            .header(header::CONTENT_TYPE, "application/json")
            .body(awzy_http::Body::from(body))
            .map_err(|e| LlmError::Failed(e.to_string()))?;
        let res =
            awzy_http::send(&self.client, req, RESPOND_TIMEOUT).await.map_err(|e| LlmError::Failed(e.to_string()))?;
        if !res.status.is_success() {
            return Err(classify(res.status, &String::from_utf8_lossy(&res.body)));
        }
        parse_response(&res.body)
    }
}

/// Vertex AI's answer to a failed request, as awzy's callers act on it.
fn classify(status: StatusCode, body: &str) -> LlmError {
    let message = format!("{status}: {}", body.chars().take(500).collect::<String>());
    match status {
        StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => LlmError::Unavailable(message),
        // No permission, the API or the model not enabled, or a model ID the project doesn't have.
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => LlmError::NotConfigured(message),
        _ => LlmError::Failed(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, StopReason};

    fn vertex(region: &str) -> Vertex {
        Vertex::new(
            ModelSettings { model: DEFAULT_MODEL.into(), effort: None },
            VertexLocation { project: "acme-awzy".into(), region: region.into() },
        )
    }

    #[test]
    fn requests_go_to_the_projects_region_or_the_global_endpoint() {
        assert_eq!(
            vertex("global").url(),
            "https://aiplatform.googleapis.com/v1/projects/acme-awzy/locations/global/publishers/anthropic/models/claude-opus-5-5:rawPredict"
        );
        assert_eq!(
            vertex("us-east5").url(),
            "https://us-east5-aiplatform.googleapis.com/v1/projects/acme-awzy/locations/us-east5/publishers/anthropic/models/claude-opus-5-5:rawPredict"
        );
    }

    #[test]
    fn a_missing_permission_or_model_is_not_configured_and_throttling_is_for_later() {
        assert!(matches!(classify(StatusCode::FORBIDDEN, "denied"), LlmError::NotConfigured(_)));
        assert!(matches!(classify(StatusCode::NOT_FOUND, "no such model"), LlmError::NotConfigured(_)));
        assert!(matches!(classify(StatusCode::TOO_MANY_REQUESTS, "quota"), LlmError::Unavailable(_)));
        assert!(matches!(classify(StatusCode::BAD_REQUEST, "bad"), LlmError::Failed(_)));
    }

    /// Calls Vertex AI for real when AWZY_TEST_VERTEX_PROJECT names a project with Claude enabled,
    /// with an access token in AWZY_TEST_VERTEX_TOKEN (`gcloud auth print-access-token`), and
    /// optionally AWZY_TEST_VERTEX_REGION; skipped otherwise.
    #[tokio::test]
    async fn answers_through_vertex() {
        let (Ok(project), Ok(token)) =
            (std::env::var("AWZY_TEST_VERTEX_PROJECT"), std::env::var("AWZY_TEST_VERTEX_TOKEN"))
        else {
            eprintln!("skipping: AWZY_TEST_VERTEX_PROJECT or AWZY_TEST_VERTEX_TOKEN is not set");
            return;
        };
        let region = std::env::var("AWZY_TEST_VERTEX_REGION").unwrap_or_else(|_| DEFAULT_REGION.into());
        let model = std::env::var("AWZY_TEST_VERTEX_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
        let vertex =
            Vertex::new(ModelSettings { model, effort: Some("low".into()) }, VertexLocation { project, region })
                .with_token(token);
        let request = ModelRequest {
            system: String::new(),
            messages: vec![Message::user("Reply with the single word: ready")],
            tools: vec![],
            max_tokens: 2048,
        };
        let r = vertex.respond(&request).await.unwrap();
        assert_eq!(r.stop, StopReason::EndTurn);
        assert!(r.text().to_lowercase().contains("ready"), "{}", r.text());
    }
}

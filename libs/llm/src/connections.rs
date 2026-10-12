//! The model services an install can reach, from its deploy configuration
//! (AWZY_LLM_CONNECTIONS). Which models to use, and how much traffic each gets, is the routing
//! ([`crate::Routing`]), a runtime setting; a route can only name a service listed here, so a
//! settings change never sends prompts somewhere the install wasn't set up to reach.

use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use hyper::{header, Request, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{anthropic, converse, gemini, openai, LlmError, ModelEntry, ModelRequest, ModelResponse, ModelSpec};

/// A model service, by how it's reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Connection {
    /// Amazon Bedrock in `region`, with the workload's AWS identity. Any Bedrock model.
    Bedrock { region: String },
    /// Vertex AI in `project`, at `region` or `global`, with the workload's Google identity. Model
    /// Gemini, Claude, and open models through chat completions.
    Vertex {
        project: String,
        #[serde(default = "global")]
        region: String,
        /// An environment variable holding an access token, for running outside GKE (a laptop's
        /// `gcloud auth print-access-token`). Unset: the metadata server.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_env: Option<String>,
    },
    /// Anything that speaks OpenAI's chat completions at `url` (its `/v1` base): OpenAI, Azure,
    /// vLLM, Ollama. The API key, if it needs one, is in the environment variable `api_key_env`.
    Openai {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        api_key_env: Option<String>,
    },
    /// Anthropic's own API, with the key in the environment variable `api_key_env`.
    Anthropic {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        api_key_env: String,
    },
}

fn global() -> String {
    "global".into()
}

/// Connections by the name routes use for them.
pub type Connections = BTreeMap<String, Connection>;

/// AWZY_LLM_CONNECTIONS, a JSON object of [`Connection`]s by name; none when unset.
pub(crate) fn from_env() -> Result<Connections, LlmError> {
    match std::env::var("AWZY_LLM_CONNECTIONS").ok().filter(|v| !v.trim().is_empty()) {
        Some(json) => serde_json::from_str(&json)
            .map_err(|e| LlmError::NotConfigured(format!("AWZY_LLM_CONNECTIONS isn't valid: {e}"))),
        None => Ok(Connections::new()),
    }
}

const VERTEX_ANTHROPIC_VERSION: &str = "vertex-2023-10-16";
const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const METADATA_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
/// A model's answer can take a while; a token shouldn't.
const RESPOND_TIMEOUT: Duration = Duration::from_secs(300);
const TOKEN_TIMEOUT: Duration = Duration::from_secs(5);

/// A connection, ready to call.
pub(crate) enum Endpoint {
    Bedrock(aws_sdk_bedrockruntime::Client),
    Vertex { http: awzy_http::Client, project: String, region: String, token: TokenSource },
    Openai { http: awzy_http::Client, url: String, key: Option<String> },
    Anthropic { http: awzy_http::Client, url: String, key: String },
}

pub(crate) enum TokenSource {
    Fixed(String),
    Metadata(Mutex<Option<(String, Instant)>>),
}

fn env_secret(var: &str) -> Result<String, LlmError> {
    std::env::var(var)
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| LlmError::NotConfigured(format!("{var} is not set")))
}

impl Endpoint {
    pub(crate) async fn connect(connection: &Connection) -> Result<Self, LlmError> {
        Ok(match connection {
            Connection::Bedrock { region } => Self::Bedrock(converse::client(region).await),
            Connection::Vertex { project, region, token_env } => Self::Vertex {
                http: awzy_http::client(),
                project: project.clone(),
                region: region.clone(),
                token: match token_env {
                    Some(var) => TokenSource::Fixed(env_secret(var)?),
                    None => TokenSource::Metadata(Mutex::new(None)),
                },
            },
            Connection::Openai { url, api_key_env } => Self::Openai {
                http: awzy_http::client(),
                url: url.trim_end_matches('/').to_string(),
                key: api_key_env.as_deref().map(env_secret).transpose()?,
            },
            Connection::Anthropic { url, api_key_env } => Self::Anthropic {
                http: awzy_http::client(),
                url: url.as_deref().unwrap_or("https://api.anthropic.com").trim_end_matches('/').to_string(),
                key: env_secret(api_key_env)?,
            },
        })
    }

    /// `request` to `entry`'s model, in the API its spec names, on this service.
    pub(crate) async fn respond(&self, entry: &ModelEntry, request: &ModelRequest) -> Result<ModelResponse, LlmError> {
        let name = entry.spec.model();
        match (self, &entry.spec) {
            (Self::Bedrock(client), ModelSpec::Converse(spec)) => converse::respond(client, spec, request).await,
            (Self::Vertex { http, project, region, token }, spec) => {
                let host = if region == "global" {
                    "aiplatform.googleapis.com".to_string()
                } else {
                    format!("{region}-aiplatform.googleapis.com")
                };
                let base = format!("https://{host}/v1/projects/{project}/locations/{region}");
                let bearer = format!("Bearer {}", token.get(http).await?);
                let headers = [(header::AUTHORIZATION.as_str(), bearer.as_str())];
                match spec {
                    ModelSpec::Gemini(s) => {
                        let url = format!("{base}/publishers/google/models/{}:generateContent", s.model);
                        gemini::parse(&post(http, &url, &headers, &gemini::body(request, s)).await?, name)
                    }
                    ModelSpec::AnthropicMessages(s) => {
                        let url = format!("{base}/publishers/anthropic/models/{}:rawPredict", s.model);
                        let body = anthropic::body(request, Some(VERTEX_ANTHROPIC_VERSION), false, s);
                        anthropic::parse(&post(http, &url, &headers, &body).await?, name)
                    }
                    // Open models (Llama, Mistral, and others), named with their publisher.
                    ModelSpec::OpenaiChat(s) => {
                        let url = format!("{base}/endpoints/openapi/chat/completions");
                        openai::parse(&post(http, &url, &headers, &openai::body(request, s)).await?, name)
                    }
                    ModelSpec::Converse(_) => Err(mismatch(entry)),
                }
            }
            (Self::Openai { http, url, key }, ModelSpec::OpenaiChat(s)) => {
                let bearer = key.as_ref().map(|k| format!("Bearer {k}"));
                let headers: Vec<(&str, &str)> =
                    bearer.as_deref().map(|b| (header::AUTHORIZATION.as_str(), b)).into_iter().collect();
                let body = openai::body(request, s);
                openai::parse(&post(http, &format!("{url}/chat/completions"), &headers, &body).await?, name)
            }
            (Self::Anthropic { http, url, key }, ModelSpec::AnthropicMessages(s)) => {
                let headers = [("x-api-key", key.as_str()), ("anthropic-version", ANTHROPIC_API_VERSION)];
                let body = anthropic::body(request, None, true, s);
                anthropic::parse(&post(http, &format!("{url}/v1/messages"), &headers, &body).await?, name)
            }
            _ => Err(mismatch(entry)),
        }
    }
}

/// A routing pairs a spec with a connection that can't serve it; validation refuses that first.
fn mismatch(entry: &ModelEntry) -> LlmError {
    LlmError::NotConfigured(format!("model {:?}'s API isn't offered by connection {:?}", entry.name, entry.connection))
}

impl TokenSource {
    /// A token good for at least another minute.
    async fn get(&self, http: &awzy_http::Client) -> Result<String, LlmError> {
        let cache = match self {
            Self::Fixed(token) => return Ok(token.clone()),
            Self::Metadata(cache) => cache,
        };
        if let Some((token, until)) = cache.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            if Instant::now() + Duration::from_secs(60) < until {
                return Ok(token);
            }
        }
        #[derive(Deserialize)]
        struct AccessToken {
            access_token: String,
            expires_in: u64,
        }
        let res = awzy_http::get(http, METADATA_TOKEN_URL, &[("Metadata-Flavor", "Google")], TOKEN_TIMEOUT)
            .await
            .map_err(|e| LlmError::NotConfigured(format!("no access token from the metadata server: {e}")))?;
        if !res.status.is_success() {
            return Err(LlmError::NotConfigured(format!("the metadata server answered {}", res.status)));
        }
        let t: AccessToken = serde_json::from_slice(&res.body).map_err(|e| LlmError::Malformed(e.to_string()))?;
        let until = Instant::now() + Duration::from_secs(t.expires_in);
        *cache.lock().unwrap_or_else(|e| e.into_inner()) = Some((t.access_token.clone(), until));
        Ok(t.access_token)
    }
}

/// POSTs `body` as JSON and returns the answer's body, or the error a failed status means.
async fn post(
    http: &awzy_http::Client,
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Result<Vec<u8>, LlmError> {
    let mut req = Request::post(url).header(header::CONTENT_TYPE, "application/json");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let bytes = serde_json::to_vec(body).map_err(|e| LlmError::Failed(e.to_string()))?;
    let req = req.body(awzy_http::Body::from(bytes)).map_err(|e| LlmError::Failed(e.to_string()))?;
    let res = awzy_http::send(http, req, RESPOND_TIMEOUT).await.map_err(|e| LlmError::Unavailable(e.to_string()))?;
    if !res.status.is_success() {
        return Err(classify(res.status, &String::from_utf8_lossy(&res.body)));
    }
    Ok(res.body.to_vec())
}

/// A failed answer, as awzy's callers act on it.
fn classify(status: StatusCode, body: &str) -> LlmError {
    let message = format!("{status}: {}", body.chars().take(500).collect::<String>());
    match status {
        StatusCode::TOO_MANY_REQUESTS
        | StatusCode::SERVICE_UNAVAILABLE
        | StatusCode::BAD_GATEWAY
        | StatusCode::GATEWAY_TIMEOUT => LlmError::Unavailable(message),
        // No permission, the API or the model not enabled, or a model ID the service doesn't have.
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => LlmError::NotConfigured(message),
        _ => LlmError::Failed(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn connections_are_named_and_say_how_each_is_reached() {
        let c: Connections = serde_json::from_value(json!({
            "aws": { "kind": "bedrock", "region": "us-east-2" },
            "gcp": { "kind": "vertex", "project": "acme" },
            "vllm": { "kind": "openai", "url": "http://vllm.models.svc:8000/v1/" },
            "azure": { "kind": "openai", "url": "https://acme.openai.azure.com/openai/v1", "api_key_env": "AWZY_LLM_KEY_AZURE" },
        }))
        .unwrap();
        assert_eq!(c["gcp"], Connection::Vertex { project: "acme".into(), region: "global".into(), token_env: None });
        assert!(matches!(&c["azure"], Connection::Openai { api_key_env: Some(k), .. } if k == "AWZY_LLM_KEY_AZURE"));
        assert!(serde_json::from_value::<Connections>(json!({ "x": { "kind": "carrier-pigeon" } })).is_err());
    }

    #[test]
    fn throttling_is_for_later_and_a_missing_permission_or_model_is_configuration() {
        assert!(matches!(classify(StatusCode::TOO_MANY_REQUESTS, "quota"), LlmError::Unavailable(_)));
        assert!(matches!(classify(StatusCode::FORBIDDEN, "denied"), LlmError::NotConfigured(_)));
        assert!(matches!(classify(StatusCode::NOT_FOUND, "no such model"), LlmError::NotConfigured(_)));
        assert!(matches!(classify(StatusCode::BAD_REQUEST, "bad"), LlmError::Failed(_)));
    }
}

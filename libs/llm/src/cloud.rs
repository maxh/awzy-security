//! The install's model service, whichever cloud it's in: callers hold a [`CloudModel`] and never
//! name a provider (ADR 0114). Bootstrap sets which one from the cloud layer's install contract.

use crate::{
    bedrock::{self, Bedrock},
    vertex::{self, Vertex, VertexLocation},
    LlmError, ModelProvider, ModelRequest, ModelResponse, ModelSettings,
};

/// The model services awzy can use.
pub enum CloudModel {
    /// Amazon Bedrock, on AWS.
    Bedrock(Bedrock),
    /// Vertex AI, on Google Cloud.
    Vertex(Vertex),
}

impl CloudModel {
    /// From the environment:
    /// - AWZY_LLM_PROVIDER: `bedrock` (the default) or `vertex`.
    /// - AWZY_LLM_MODEL: the model ID in that service; its Claude Opus by default.
    /// - AWZY_LLM_EFFORT: how hard the model works, when not the model's default.
    /// - Bedrock's region is AWS_REGION. Vertex AI's project is AWZY_LLM_PROJECT, and its region
    ///   AWZY_LLM_REGION (`global` by default).
    pub async fn from_env() -> Result<Self, LlmError> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let settings = |default: &str| ModelSettings {
            model: var("AWZY_LLM_MODEL").unwrap_or_else(|| default.to_string()),
            effort: var("AWZY_LLM_EFFORT"),
        };
        match var("AWZY_LLM_PROVIDER").as_deref().unwrap_or("bedrock") {
            "bedrock" => Ok(Self::Bedrock(Bedrock::from_env(settings(bedrock::DEFAULT_MODEL)).await)),
            "vertex" => {
                let project = var("AWZY_LLM_PROJECT")
                    .ok_or_else(|| LlmError::NotConfigured("AWZY_LLM_PROJECT is not set".into()))?;
                let region = var("AWZY_LLM_REGION").unwrap_or_else(|| vertex::DEFAULT_REGION.to_string());
                Ok(Self::Vertex(Vertex::new(settings(vertex::DEFAULT_MODEL), VertexLocation { project, region })))
            }
            other => Err(LlmError::NotConfigured(format!("AWZY_LLM_PROVIDER is bedrock or vertex, not {other:?}"))),
        }
    }
}

impl CloudModel {
    /// The model's ID in its service, for logs and for what a run records it used.
    pub fn model(&self) -> &str {
        match self {
            Self::Bedrock(m) => m.model(),
            Self::Vertex(m) => m.model(),
        }
    }
}

impl ModelProvider for CloudModel {
    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, LlmError> {
        match self {
            Self::Bedrock(m) => m.respond(request).await,
            Self::Vertex(m) => m.respond(request).await,
        }
    }
}

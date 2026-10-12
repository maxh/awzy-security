//! Claude on Amazon Bedrock, in the install's own AWS account (ADR 0048).
//!
//! Requests go through Bedrock Runtime's InvokeModel with Claude's Messages API body. AWS
//! credentials come from the usual chain, which in the cluster is the service's workload
//! identity (EKS Pod Identity), so there's no key to manage. The client enables Claude in their
//! Bedrock account and region; until then requests fail with [`LlmError::NotConfigured`].

use crate::{
    messages::{parse_response, request_body},
    LlmError, ModelProvider, ModelRequest, ModelResponse, ModelSettings,
};
use aws_config::BehaviorVersion;
use aws_sdk_bedrockruntime::{error::ProvideErrorMetadata, primitives::Blob, Client};
use aws_smithy_http_client::tls;
use aws_smithy_types::error::display::DisplayErrorContext;

/// The Bedrock model ID for Claude Opus 5.5. Some accounts need a regional inference profile
/// instead (e.g. `us.anthropic.claude-opus-5-5`); set AWZY_LLM_MODEL.
pub const DEFAULT_MODEL: &str = "anthropic.claude-opus-5-5";

/// Bedrock's version tag for Claude's Messages API body.
const ANTHROPIC_VERSION: &str = "bedrock-2023-05-31";

pub struct Bedrock {
    client: Client,
    config: ModelSettings,
}

impl Bedrock {
    /// With credentials and region (AWS_REGION) from the environment.
    pub async fn from_env(config: ModelSettings) -> Self {
        let https = aws_smithy_http_client::Builder::new()
            .tls_provider(tls::Provider::Rustls(tls::rustls_provider::CryptoMode::Ring))
            .build_https();
        let aws = aws_config::defaults(BehaviorVersion::latest()).http_client(https).load().await;
        Self { client: Client::new(&aws), config }
    }

    /// The model's ID.
    pub fn model(&self) -> &str {
        &self.config.model
    }
}

impl ModelProvider for Bedrock {
    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, LlmError> {
        let body = request_body(request, ANTHROPIC_VERSION, self.config.effort.as_deref());
        let out = self
            .client
            .invoke_model()
            .model_id(&self.config.model)
            .content_type("application/json")
            .accept("application/json")
            .body(Blob::new(serde_json::to_vec(&body).map_err(|e| LlmError::Failed(e.to_string()))?))
            .send()
            .await
            .map_err(|e| {
                // The whole chain: credential and network failures carry no service error code.
                let detail = DisplayErrorContext(&e).to_string();
                classify(e.into_service_error(), detail)
            })?;
        parse_response(out.body().as_ref())
    }
}

fn classify<E: ProvideErrorMetadata>(e: E, detail: String) -> LlmError {
    let message = match e.code() {
        Some(code) => format!("{code}: {}", e.message().unwrap_or(&detail)),
        None => detail,
    };
    match e.code() {
        Some(
            "ThrottlingException"
            | "ServiceQuotaExceededException"
            | "ServiceUnavailableException"
            | "ModelNotReadyException",
        ) => LlmError::Unavailable(message),
        Some("AccessDeniedException" | "ResourceNotFoundException" | "UnrecognizedClientException") => {
            LlmError::NotConfigured(message)
        }
        _ => LlmError::Failed(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, StopReason};

    /// Calls Bedrock for real when AWZY_TEST_BEDROCK_MODEL names a model this machine's AWS
    /// credentials can use (e.g. with AWS_PROFILE and AWS_REGION set); skipped otherwise.
    #[tokio::test]
    async fn answers_through_bedrock() {
        let Ok(model) = std::env::var("AWZY_TEST_BEDROCK_MODEL") else {
            eprintln!("skipping: AWZY_TEST_BEDROCK_MODEL is not set");
            return;
        };
        let bedrock = Bedrock::from_env(ModelSettings { model, effort: Some("low".into()) }).await;
        let request = ModelRequest {
            system: String::new(),
            messages: vec![Message::user("Reply with the single word: ready")],
            tools: vec![],
            max_tokens: 2048,
        };
        let r = bedrock.respond(&request).await.unwrap();
        assert_eq!(r.stop, StopReason::EndTurn);
        assert!(r.text().to_lowercase().contains("ready"), "{}", r.text());
    }
}

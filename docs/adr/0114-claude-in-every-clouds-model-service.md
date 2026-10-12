# 0114. Claude in every cloud's own model service, behind one interface

- Status: Accepted
- Date: 2026-10-12
- Extends: [0048](0048-awzy-bot-on-the-installs-own-model-service.md) (Awzy Bot on the install's own model service), to Google Cloud
- Builds on: [0015](0015-cloud-agnostic-install.md) (the install contract), [0071](0071-investigations-run-on-the-reasoners-flags-and-are-kept-in-awzy-context.md) (investigations)

## Context

Awzy Bot and the reasoner's investigations call Claude in the install's own cloud, so prompts
never leave the client's account (0048). Only Amazon Bedrock was built: services constructed a
Bedrock client by name, the chart passed only an AWS region, and bootstrap turned both features
off on any other cloud. A Google Cloud install ran without them.

## Decision

- **Callers hold a `CloudModel`** (`libs/llm`), which picks the provider from the environment
  (`AWZY_LLM_PROVIDER`: `bedrock` or `vertex`) and implements the same `ModelProvider` trait.
  No service names a provider.
- **Claude on Vertex AI for Google Cloud.** Requests go to Vertex AI's `rawPredict` for
  Anthropic's models, in the install's project, with a token from the GKE metadata server as the
  workload's own identity. Vertex AI takes Claude's Messages API body like Bedrock does, so both
  share it (`messages.rs`), with each service's version tag.
- **The cloud layer says which service** in the install contract's `model_service`: the provider,
  its region, and for Vertex AI the project. Bootstrap passes it to the chart's `modelService`
  without branching on the cloud, and turns the features on only where a cloud has one. The GCP
  layer enables the Vertex AI API and lets the portal's and the services' identities call models
  (`roles/aiplatform.user`); its region is `global` by default.
- **The client still enables Claude** in their cloud's catalog: Bedrock's model access on AWS,
  Vertex AI's Model Garden on Google Cloud. Until then, calls fail as not configured, and the
  features say so.

Rejected: calling Anthropic's API directly. Prompts would leave the client's account. Rejected: an
OpenAI-compatible gateway in front of every cloud. It's another service to run, and each cloud's
own model service already speaks Claude's format.

## Consequences

- Azure's model service can follow behind the same trait, with a `model_service` from an Azure
  layer.
- A chart from before `modelService` still reads each feature's own region, and bootstrap keeps
  sending it.
- The model's ID differs by service (`anthropic.claude-opus-5-5` on Bedrock, `claude-opus-5-5` on
  Vertex AI), so `llm_model` is per install, as before.

# 0048. Awzy Bot runs on the install's own model service, Bedrock first, and only proposes

- Status: Accepted
- Date: 2026-10-09
- Settles part of the platform plan's open question on model providers

## Context

awzy's first AI feature is **Awzy Bot**, an assistant in the client portal: it explains
decisions ("why was bob denied?"), looks into what an accessor did, and drafts policy changes in
Cedar from a plain description. Classification, justification checks, and investigations
(Phases 2 to 4) will use the same model access.

What it reads is the client's data: decisions, people, resources, policies. 0009 keeps awzy out
of that data, so prompts can't go to a model account awzy runs. The plan's per-cloud table
already names each cloud's model service (Bedrock, Vertex AI, Azure OpenAI) for this.

## Decision

- **The model runs in the client's own cloud account,** called with the service's workload
  identity: no API keys, the client's quota, budget, and data controls. **Amazon Bedrock is the
  first adapter** (`libs/llm`), Claude Opus 5.5 by default (`AWZY_LLM_MODEL`, effort with
  `AWZY_LLM_EFFORT`). Vertex AI and Azure follow behind the same `ModelProvider` trait.
- **Requests use Claude's Messages API shape** (blocks, tool use) through Bedrock Runtime's
  InvokeModel, from Rust via the AWS SDK. Blocks awzy doesn't interpret, such as the model's
  thinking, are passed back unchanged, and conversations are only appended to.
- **Model output is only ever a proposal.** Awzy Bot's tools read and explain, and the one tool
  that changes anything creates a policy draft, which awzy-policy validates like any other and a
  person publishes. Every tool call is checked with the PDP as the signed-in person (0020), so
  the bot can't see or do more than they can. Drafts record that Awzy Bot wrote them, for whom.
- **Never on the decision path:** awzy-pdp doesn't depend on `libs/llm`.
- **Content the model reads is untrusted.** Resource names, justifications, and log lines can be
  written by an attacker, which is another reason tools can't act.
- A refusal (`stop_reason: refusal`) is shown to the person as such; Bedrock has no server-side
  fallback to another model.

Rejected: the Anthropic API or Claude Platform on AWS from awzy's own account, because client
data would leave the client's account (0009). Rejected for now: a self-hosted model, which the
plan keeps as an option for clients who won't use their cloud's model service.

## Consequences

- An install needs Claude enabled in its Bedrock account and region, and its portal's workload
  role allowed `bedrock:InvokeModel` on that model; the Terraform module and preflight check
  cover it. Without it, Awzy Bot says it isn't set up.
- The model bill lands in the client's account. A per-install token budget, which the plan
  already calls for, has to exist before Awzy Bot is on by default.
- The AWS SDK adds a C crypto library (aws-lc) to the build. It builds for macOS and for the
  linux/amd64 images under Bazel.

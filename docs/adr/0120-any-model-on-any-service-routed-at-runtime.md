# 0120. Any model on any service, routed at runtime

- Status: Accepted
- Date: 2026-10-12
- Amends: [0048](0048-awzy-bot-on-the-installs-own-model-service.md) (Awzy Bot on the install's own model service): the model is no longer fixed to Claude, or to the cloud's own service
- Builds on: [0015](0015-cloud-agnostic-install.md) (the install contract), [0071](0071-investigations-run-on-the-reasoners-flags-and-are-kept-in-awzy-context.md) (investigations), [0089](0089-the-outage-fallback-is-set-in-the-portal-and-signed.md) (settings awzy-policy keeps and signs)

## Context

Awzy Bot and the reasoner's investigations called Claude on Amazon Bedrock. Services built a
Bedrock client by name, the request and response types were Claude's own, the model was fixed
at deploy time in environment variables, and a Google Cloud install ran without either feature.
Clients want to choose: Gemini where it's their standard, an open-weight model they host, Claude
where they have it. They also want to try a new model on part of the traffic before moving to
it, without a redeploy.

## Decision

- **Callers speak neutral types and hold a router** (`libs/llm`). A conversation of a feature
  (`awzy_bot`, `investigations`) gets its model from the `ModelRouter` and keeps it every turn.
  What only one model understands (Claude's thinking, Gemini's thought signatures) travels as
  opaque blocks tagged with their format, sent back to that format only.
- **A model is described in its API's own terms.** A catalog entry is a name, a connection, and a
  spec, a discriminated union on the API it speaks:
  - `converse`: Bedrock's Converse, for any Bedrock model, with `model_id`, `cache_points`, and
    `additional_model_request_fields`.
  - `anthropic_messages`: Claude on Vertex AI or Anthropic's API, with `model`, `effort`,
    `thinking_budget_tokens`, and `prompt_caching`.
  - `gemini`: Gemini on Vertex AI, with `model`, `thinking_level`, and `temperature`.
  - `openai_chat`: chat completions, for OpenAI, Azure, vLLM, Ollama, and open models on
    Vertex AI, with `model`, `reasoning_effort`, and `temperature`.

  Unknown fields are refused, so a typo fails when the setting is saved. Each spec has an `extra`
  object passed to the model as is, for settings awzy hasn't typed yet. One adapter per API
  translates.
- **What the install can reach is deploy configuration; which model to use is a runtime
  setting.**
  - **Connections** come from the cloud layer and bootstrap: the cloud's own service (the
    install contract's `model_connection`) and any others the client adds, with API keys from
    Secrets.
  - **The routing** is a catalog plus a route per feature: weighted targets, to dial traffic up
    or down, and an ordered fallback for when a model is busy, down, or not set up.
  - **Where it's kept:** awzy-policy keeps the routing and signs it like the outage fallback
    (0089), and the portal and the reasoner follow it without restarting. Until it's first
    set, bootstrap's starting routing applies.
- **A setting can't send prompts somewhere new.** A routing names only connections the install
  already has, and is refused otherwise. Changing it is an admin action, decided by policy, and
  recorded.
- **Google Cloud reaches Vertex AI as the workload's own identity.** The GCP layer enables the
  API and grants the portal and the services `roles/aiplatform.user`.

Rejected:
- **A single model per install in environment variables.** Changing it means a redeploy, and
  there's no way to try a model on part of the traffic.
- **One flat model config with a free-form parameter map.** Every provider's settings would be
  flattened into one vocabulary, with nothing checked until a request failed.
- **Choosing the model per request in Cedar.** Routing is about cost, quality, and residency,
  not authorization. Policy decides who may change it.

## Consequences

- **The evals decide fitness.** `authz/demo/evals` runs against any route, which is how a
  model is judged ready for the investigations route.
- **Phases:**
  1. The router, the adapters, and deploy-time connections (this change).
  2. The routing as a signed setting in awzy-policy, followed by the services.
  3. A portal page with each model's latency, errors, tokens, and eval results.
- **A new provider setting** is typed in a small code change; `extra` covers it until then.

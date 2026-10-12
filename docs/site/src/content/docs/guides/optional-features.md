---
title: Turning on optional features
description: Awzy Bot, investigations, and Slack notifications, each off until you turn it on.
---

Three features reach outside your cluster, so each is off until you turn it on. All of them use
services in your own accounts: your cloud's model service and your own Slack. Nothing goes
through awzy.

Each is a variable in the cluster layer's Terraform (`deploy/terraform/bootstrap`), applied as in
[Installing awzy](/guides/install/), step 5.

## Awzy Bot and investigations: a model of your choosing

Both work with any model you can reach: your cloud's own model service, in the install's account,
with the permissions the cloud layer already gives awzy's workloads, or another service you add,
such as a model you host. On your cloud's own service, prompts and answers stay in your account.

Set the model in the API it speaks, in that API's own terms, with `llm_model`:

**On AWS:** any model on Amazon Bedrock, through Bedrock's Converse API. First request access to
the model in the install account's Bedrock console, in the install's region. Some models are
reached through a cross-region inference profile, an ID starting `us.`:

```hcl
llm_model = { api = "converse", model_id = "us.anthropic.claude-opus-5-5", cache_points = true }
```

**On Google Cloud:** Gemini needs nothing beyond what the cloud layer sets up (it turns on the
Vertex AI API and lets awzy's workloads call it). Claude, or another partner model, is enabled
first in the project's Vertex AI Model Garden. Requests go to Vertex AI's `global` endpoint; to
keep them in one region, set the cloud layer's `model_region` (for example `us-east5`).

```hcl
llm_model = { api = "gemini", model = "gemini-2.5-pro", thinking_level = "low" }
# or
llm_model = { api = "anthropic_messages", model = "claude-opus-5-5" }
```

**Another service:** add it with `llm_connections` and describe its models with `llm_routing`.
Anything that speaks OpenAI's chat completions works (vLLM, Ollama, Azure OpenAI), as does
Anthropic's API. An API key comes from a Secret, through `llm_api_key_secrets`:

```hcl
llm_connections     = { vllm = { kind = "openai", url = "http://vllm.models.svc:8000/v1", api_key_env = "AWZY_LLM_KEY_VLLM" } }
llm_api_key_secrets = { AWZY_LLM_KEY_VLLM = { secret = "vllm-key" } }
llm_routing = {
  models = [
    { name = "gemini", connection = "cloud", spec = { api = "gemini", model = "gemini-2.5-pro" } },
    { name = "llama", connection = "vllm", spec = { api = "openai_chat", model = "meta-llama/Llama-4-Maverick" } },
  ]
  routes = {
    awzy_bot       = { targets = [{ model = "gemini", weight = 9 }, { model = "llama", weight = 1 }], fallback = ["llama"] }
    investigations = { targets = [{ model = "gemini" }] }
  }
}
```

A route splits a feature's conversations between models by weight, so you can try a model on a
share of them, and falls back in order when a model is busy or down. A routing can only use the
connections the install has, so it never sends prompts anywhere you didn't set up.

### Awzy Bot

The assistant on the client portal's Policies page, which explains decisions and drafts
policies. It only ever drafts: a person publishes.

```hcl
awzy_bot = true
```

### Investigations

A model looks into the principals awzy-reasoner flags. See [Investigations](/concepts/investigations/).

```hcl
investigations = true
```

Two limits tune them:

```hcl
investigation_budget = {
  monthly_tokens    = 2000000
  privileged_groups = ["it", "security"]
}
```

- **`monthly_tokens`** (2,000,000 by default): the most a calendar month spends. When it's spent,
  no new investigation starts until the next month.
- **`privileged_groups`** (`it` and `security` by default): members of these groups are only ever
  proposed for containment, never contained automatically. Name the groups as your IdP sends them.

Turn on [automated containments](/concepts/challenges-and-containment/#automated-containments)
separately, on the Enforcement page, once you trust what they'd do. Until then, a containment an
investigation recommends is only recorded.

## Slack notifications

People get a direct message when something waits for them: a reason or a security key to give,
or an approval to make. Requesters hear when their request is decided. Your security team's
channel hears about approvals, containments, and every break-glass rollback. Messages link into the client portal, and never
include document contents or the evidence behind a containment.

1. **Create a Slack app** in your workspace (api.slack.com/apps). Give its bot token the
   `chat:write` and `users:read.email` scopes, install it, and copy the bot token (`xoxb-…`).
2. **Pick the security team's channel,** invite the app to it, and copy its channel ID (from the
   channel's details).
3. **Set the variable:**

   ```hcl
   slack = {
     token            = "xoxb-…"
     security_channel = "C0123ABCD"
   }
   ```

   Keep the token out of version control, for example in a `.tfvars` file you don't commit. It's
   stored as a Kubernetes Secret in the `awzy-system` namespace.

awzy finds people in Slack by email: the primary email your IdP provisions for them over SCIM
(else their work email), or their user name when it is an email address. Someone with neither
doesn't get direct messages; the security team's channel still does. Links use
the client portal's public URL, so set its hostname first.

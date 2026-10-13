---
title: Turning on optional features
description: Awzy Bot, investigations, and notifications in Slack or by email, each off until you turn it on.
---

Three features reach outside your cluster, so each is off until you turn it on. All of them use
services in your own accounts: your cloud's model service, and your own Slack or mail relay.
Nothing goes through awzy.

Each is a variable in the cluster layer's Terraform (`deploy/terraform/bootstrap`), applied as in
[Installing awzy](/guides/install/), step 5.

## Awzy Bot and investigations: a model of your choosing

Both work with any model you can reach: your cloud's own model service, in the install's account,
with the permissions the cloud layer already gives awzy's workloads, or another service you add,
such as a model you host. On your cloud's own service, prompts and answers stay in your account.

Set the model in the API it speaks, in that API's own terms, with `llm_model`:

**On AWS:** any model on Amazon Bedrock, through Bedrock's Converse API. First request access to
the model in the install account's Bedrock console, in the install's region. Some models are
reached through a cross-region inference profile, an ID starting `us.`. Bedrock serves those
from other regions, so if a service control policy denies requests outside the install's region,
exempt calls through such a profile: add `"ArnNotLike": { "bedrock:InferenceProfileArn":
"arn:aws:bedrock:*:*:inference-profile/us.*" }` to the deny statement's conditions.

```hcl
llm_model = { api = "converse", model_id = "us.anthropic.claude-opus-5-5", cache_points = true }
```

**On Google Cloud:** Gemini needs nothing beyond what the cloud layer sets up (it turns on the
Vertex AI API and lets awzy's workloads call it). Claude, or another partner model, is enabled
first in the project's Vertex AI Model Garden. Requests go to Vertex AI's `global` endpoint; to
keep them in one region, set the cloud layer's `model_region` (for example `us-east5`).

```hcl
llm_model = { api = "gemini", model = "gemini-2.5-pro" }
# or
llm_model = { api = "anthropic_messages", model = "claude-opus-5-5" }
```

**Another service:** add it with `llm_connections` and describe its models with `llm_routing`.
Anything that speaks OpenAI's chat completions works (vLLM, Ollama, Azure OpenAI), as does
Anthropic's API, and Vertex AI from another cloud. An API key goes in `llm_api_keys`, which
bootstrap keeps in a Secret, or comes from a Secret you manage, through `llm_api_key_secrets`:

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

Vertex AI from outside Google Cloud takes an API key bound to a service account that may call
models (`roles/aiplatform.user`), made with `gcloud services api-keys create --service-account=…`:

```hcl
llm_connections = { vertex = { kind = "vertex", project = "your-project", api_key_env = "AWZY_LLM_KEY_VERTEX" } }
llm_api_keys    = { AWZY_LLM_KEY_VERTEX = var.vertex_api_key }
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

## Notifications

People are told when something waits for them: a reason or a security key to give, or an
approval to make. Requesters hear when their request is decided. Your security team hears about
approvals, containments, and every break-glass rollback. Messages link into the client portal,
and never include document contents or the evidence behind a containment. Send them through
Slack, email, or both: with both, each message goes out on each.

### In Slack

People get a direct message, and the security team a post in its channel.

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

### By email

For people and teams without Slack. awzy sends plain-text email through an SMTP relay you
provide, such as your mail provider's, or Amazon SES's SMTP interface on AWS.

1. **Get the relay's address and credentials,** and the sender address it lets you send from.
   The connection must be encrypted: TLS from the start (`smtps://`, usually port 465), or
   STARTTLS (`smtp://` with `?tls=required`, usually port 587).
2. **Pick the security team's address,** a mailbox or a list.
3. **Set the variable:**

   ```hcl
   email = {
     relay_url        = "smtps://USERNAME:PASSWORD@smtp.example.com:465"
     from             = "awzy <awzy@example.com>"
     security_address = "security-team@example.com"
   }
   ```

   Percent-encode any special characters in the user name or password.

Keep the Slack token and the relay's credentials out of version control, for example in a
`.tfvars` file you don't commit. Each is stored as a Kubernetes Secret in the `awzy-system`
namespace.

awzy finds people by email: the primary email your IdP provisions for them over SCIM (else their
work email), or their user name when it is an email address. Someone with neither isn't messaged
directly; the security team still is. Links use the client portal's public URL, so set its
hostname first.

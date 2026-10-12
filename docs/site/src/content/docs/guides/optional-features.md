---
title: Turning on optional features
description: Awzy Bot, investigations, and Slack notifications, each off until you turn it on.
---

Three features reach outside your cluster, so each is off until you turn it on. All of them use
services in your own accounts: your cloud's model service and your own Slack. Nothing goes
through awzy.

Each is a variable in the cluster layer's Terraform (`deploy/terraform/bootstrap`), applied as in
[Installing awzy](/guides/install/), step 5.

## Awzy Bot and investigations: Claude in your cloud

Both use Claude through your cloud's own model service, in the install's account, with the
permissions the cloud layer already gives awzy's workloads. Prompts and answers stay in your
account. That's Amazon Bedrock on AWS and Vertex AI on Google Cloud; Azure's is planned behind
the same interface.

First, enable Anthropic's Claude models in your cloud's model service. If the default model ID
isn't reachable there, set the one that is with `llm_model`.

**On AWS:** request access to Claude in the install account's Bedrock console, in the install's
region. Some accounts can only reach Claude through a cross-region inference profile, a model ID
starting `us.`:

```hcl
llm_model = "us.anthropic.claude-opus-5-5"
```

**On Google Cloud:** enable Claude Opus in the install project's Vertex AI Model Garden (Model
Garden, then Claude Opus, then Enable). The cloud layer turns on the Vertex AI API and lets
awzy's workloads call it. Requests go to Vertex AI's `global` endpoint, which serves Claude from
any region with capacity; to keep them in one region that offers Claude, set the cloud layer's
`model_region` (for example `us-east5`). To pin a model version:

```hcl
llm_model = "claude-opus-5-5@YYYYMMDD"
```

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

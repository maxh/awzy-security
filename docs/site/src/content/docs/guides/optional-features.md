---
title: Turning on optional features
description: Awzy Bot, investigations, and Slack notifications, each off until you turn it on.
---

Three features reach outside your cluster, so each is off until you turn it on. All of them use
services in your own accounts: your cloud's model service and your own Slack. Nothing goes
through awzy.

Each is a variable in the cluster layer's Terraform (`deploy/terraform/bootstrap`), applied as in
[Installing awzy](/guides/install/), step 5.

## Awzy Bot and investigations: Claude on Bedrock

Both use Claude through Amazon Bedrock in your install's AWS account, with the permissions the
install's Terraform already gives awzy's workloads.

First, in the install account's Bedrock console, request access to Anthropic's Claude models in
the install's region. Some accounts can only reach them through a cross-region inference profile
(a model ID starting `us.`). If so, set that ID:

```hcl
llm_model = "us.anthropic.claude-opus-5-5"
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
channel hears about approvals and containments. Messages link into the client portal, and never
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

awzy finds people in Slack by email, using their IdP user name. Someone whose user name isn't an
email address doesn't get direct messages; the security team's channel still does. Links use
the client portal's public URL, so set its hostname first.

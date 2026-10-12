---
title: awzy and your existing authorization
description: "What awzy adds to your IdP, cloud IAM, and your apps' own permissions, and how to adopt it a step at a time."
---

You already have authorization. Your IdP decides who can sign in, your cloud's IAM decides which
workloads can call which APIs, and each of your apps has its own permissions, perhaps on a policy
engine. awzy doesn't replace any of them. It adds one more check, made the same way everywhere:
may this person, or this agent acting for them, take this action on this resource, right now?

## What each one answers

| System                               | The question it answers                       | What it leaves open                                                                                    |
| ------------------------------------ | --------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Your IdP (SSO, MFA)                  | Who are you, when you sign in?                | Everything after sign-in: a session can do whatever its permissions allow, for hours.                  |
| Cloud IAM                            | May this workload call this API?              | The person behind an agent, and which record it touches.                                               |
| Your apps' own permissions           | May this user see this record, in this app?   | Each app decides alone, without your other apps' view, current risk, or the person an agent works for. |
| A policy engine (OPA, Cedar, Cerbos) | Given these attributes, allow or deny?        | The attributes, the enforcement, outcomes between allow and deny, and the log.                         |
| Access reviews (IGA, PAM)            | Should this person keep this standing access? | What they do with it in between reviews.                                                               |

## What awzy adds

- **A decision for every action, on the resource it touches**, from the same policies whether
  the call comes through an MCP tool, your own app, or the client portal.
- **Delegation**: an agent acting for a person is checked as itself and as the person, and gets
  the stricter answer. It never does more than the person could.
- **What's true now**: the person's groups from your IdP, the resource's sensitivity and owner,
  whether they're assigned to the work, and a risk score from what they've been doing.
- **Outcomes between allow and deny**: a justification, a security key, or someone's approval,
  and containment when risk is high.
- **One decision log** across systems, and **monitor mode**, so you see what awzy would block
  before it blocks anything.

## Where it sits

awzy sits beside your apps' authorization, not in front of it or instead of it. Your app keeps
deciding what its customers may do with their data. awzy covers your own staff and the AI agents
that act for them, and a call goes ahead only if both allow it:

1. Your app checks its own permissions, as it does today.
2. It asks awzy, through the [TypeScript SDK](/guides/typescript-sdk/), the
   [decision API](/reference/decision-api/), or [awzy-gateway](/guides/mcp-gateway/) for MCP tools.
3. It applies the stricter answer.

awzy isn't an IdP, isn't the authorization your customers' users see, and doesn't replace your
cloud's IAM.

## Adopting it a step at a time

Each step is useful on its own, and nothing is blocked until you choose to enforce.

1. **Watch.** Put your agents' tool calls through awzy-gateway, or check your app's operations
   with the SDK, with the install in monitor mode. Every decision is recorded as what would have
   happened; nothing changes for anyone. See [monitor, then enforce](/concepts/monitor-and-enforce/).
2. **Describe your own data.** Add your app's entity types (customers, tenants, invoices) to the
   policy schema, and write policies over the attributes your app sends for them. See
   [writing policies](/guides/writing-policies/#your-apps-own-data).
3. **Enforce narrowly.** Switch one action or one tool to enforce, such as agents' writes, while
   the rest stays in monitor. Ask for a challenge rather than denying where a person can vouch for
   the work; an agent with nobody in its session waits for the answer and retries.
4. **Bring your app's own agents.** If your app creates agents and signs their tokens, trust it as
   an issuer and let it register and retire them through the API. See
   [agent builders](/workflows/agent-builders/#when-your-app-mints-its-own-agents).
5. **Limit lists and searches.** Ask awzy which records an agent may see and turn the answer into
   a filter for your own query, so "this agent sees only these customers" holds row by row. See
   [query plans](/reference/decision-api/#query-plans).
6. **Move rules where they belong.** Rules about your own staff and agents that are scattered
   through app code can move into awzy's policies, reviewed and versioned in one place. Your
   customers' permissions stay in your app.

## If you already use a policy engine

awzy's policies are Cedar. Cedar policies you already have carry over largely as they are; rules
in another language, such as Rego, are rewritten. Run both side by side with awzy in monitor
mode, and compare their answers in the decision log before you rely on awzy's. A draft policy
bundle can be replayed against recent requests before it's published, so you see what a change
would do first.

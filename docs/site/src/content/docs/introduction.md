---
title: What awzy is
description: Beyond Zero authorization for people and AI agents.
---

awzy decides whether one accessor may take one action on one resource, every time it happens. It
implements the "Beyond Zero" model from Google's paper of that name: zero trust stops at the front
door, so authorization has to follow every action after it, for people and for the AI agents
that now act on their behalf.

:::caution[Early]
awzy is in active development and not yet generally available. These pages describe what's built
today; the [platform plan](https://github.com/maxh/awzy-security/blob/main/docs/platform-plan.md)
says what's coming.
:::

## The pieces

| Service | What it does |
| --- | --- |
| **awzy-pdp** | The policy decision point. Answers `Check(subject, action, resource)` with allow, deny, or challenge, in milliseconds. |
| **awzy-gateway** | An enforcement point you don't have to code: a proxy for MCP tool calls and HTTP APIs. |
| **awzy-policy** | Versioned, signed Cedar policy bundles: drafts, dry runs, publishing, and break-glass rollback. |
| **awzy-context** | Who and what: people and groups from your IdP over SCIM, registered service accounts, and resources with their sensitivity. |
| **awzy-challenge** | Challenges (justification, security key, approval) and containments. |
| **awzy-intake** and **awzy-reasoner** | Take in activity from app and audit logs, and score each person's and agent's risk from it. |
| **Client portal** | Where your security team reviews decisions, publishes policy, answers challenges, and manages the directory. |

## How it runs

Everything runs in your own cloud account, one install per client: awzy never sees your data, and
nothing about your people or resources leaves your install. See the
[security model](/reference/security-model/) and [installing awzy](/guides/install/).

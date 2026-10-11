---
title: People, agents, and delegation
description: Humans, service accounts, and the three ways software can act in your systems.
---

awzy has two kinds of principal:

- **`Human`**: a person, identified by your IdP. Their groups and whether they're still active
  come from your IdP over SCIM.
- **`ServiceAccount`**: anything else with its own credentials: a backend service, a scheduled
  job, or an AI agent. Each is registered in awzy-context with an accountable owner, and marked
  as AI or not. A service account awzy doesn't know is denied before any policy runs.

What awzy knows about an agent depends on the token it presents. There are three cases, and awzy
can tell the first two apart from the third only by what's in the token.

## An agent with its own identity, acting for a person

The agent gets a token that names both of them: an OAuth token exchange (RFC 8693), with the
person as `sub` and the agent as `act.sub`. awzy checks twice, once as the agent and once as the
person, and the stricter verdict wins, so **an agent never gets more than the person it acts
for**, and never more than its own registration allows.

Because awzy knows an agent is asking:

- policies can treat it differently, such as asking for an approval when its person is away;
- challenges go to the person, and the agent can't answer them: the client portal refuses a
  justification, a security key, or an approval from a token that acts for someone;
- each is judged on its own risk score.

This is the case to aim for. It needs an IdP that can exchange tokens for your agents.

## An agent on its own authority

A scheduled job or an unattended agent uses its own client-credentials token. The service
account is the principal, and its challenges go to its owner. AI acting on its own never touches
`crown_jewels` under the base policies.

## An agent using a person's own credentials

Coding agents and desktop assistants, such as Claude Code, usually run with whatever their person
has signed them into: the person's token for an MCP server, their CLI logins, their cloud
credentials. A token like that has no `act` claim, so **to awzy, the agent is the person**. The
check's principal is the `Human`, and nothing in the token says software made the request.

What follows from that:

- **It gets exactly what the person gets.** The delegation rule holds trivially, but nothing
  narrows it further.
- **awzy assumes the person is there.** `human_present` is true for a person unless the token
  says otherwise, even when the agent runs unattended.
- **Only what passes an enforcement point is checked.** Tool calls through awzy-gateway, and calls
  to your apps that ask the PDP, are decided. The agent's shell commands, git pushes, and cloud
  CLI calls never reach awzy.
- **A justification doesn't prove a person answered.** The client portal accepts the person's
  token, and in a standard install awzy-gateway and the portal accept the same tokens. An agent
  holding that token can answer a justification challenge, or approve a request the person may
  approve, through the portal's API.

What still holds:

- **A security key does.** A verification challenge needs a touch on the person's key, which no
  software can make. That's why the base policies ask for one before a high-risk operation when
  risk is elevated, and before everything when risk is high.
- **The agent's activity counts against the person.** It feeds their [risk score](/concepts/risk/),
  so an agent reading far more than its person usually does raises the score that triggers those
  challenges.
- **Calls through awzy-gateway say so.** Every check from awzy-gateway carries
  `context.client == "mcp"`, so a `Human` principal arriving that way is software holding the
  person's token. A policy can use that, for example to ask for a security key for anything
  sensitive:

  ```cedar
  @id("agents-with-human-credentials-verify")
  @challenge("verification")
  forbid (principal is Human, action == Action::"call", resource is McpTool)
  when { context has client && context.client == "mcp" &&
         (resource.sensitivity == "confidential" || resource.sensitivity == "crown_jewels") };
  ```

  awzy-gateway doesn't tell MCP clients apart yet: every one is `mcp`.

Where you can, give agents their own identity and a token exchange instead, so policies, risk, and
challenges can tell them from their people. See [Agent builders](/workflows/agent-builders/).

## Who answers challenges

Whether a person is there to answer a challenge is part of the request (`human_present`): from
the token's `awzy_human_present` claim, else true for a person and false for a service account.
An agent working while its person is away can't be verified in the moment, so policy can ask for
an approval instead.

---
title: Agent builders
description: "Building AI agents and services that work under awzy: registering them, acting for people, and handling challenges."
---

You build the AI agents and backend services that act in your company's systems. awzy checks
every action they take, on the resource it touches.

## 1. Register the service account

Every agent or service gets a **service account** on the client portal's Directory page:

- **ID:** as its tokens name it: their `sub`, or `act.sub` when it acts for a person.
- **Owner:** the person accountable for it. They answer its challenges when it acts on its own.
- **AI agent:** whether it's an AI system. Policies can treat AI differently, for example never
  letting an unattended agent touch `crown_jewels`.
- **Groups:** the groups policies should see it in.

An account that isn't registered is denied everything.

## 2. Act for a person when there is one

When your agent works for someone, get a token that says so: an OAuth token exchange
(RFC 8693) with the person as `sub` and your agent as `act.sub`. awzy then checks both the agent and
the person, and the stricter verdict wins, so your agent never gets more than the person could.
Challenges go to that person.

When it works on its own, such as a scheduled job, use its own client-credentials token. Then
challenges go to its owner. See [People, agents, and delegation](/concepts/principals-and-delegation/).

Set the token's `awzy_human_present` claim to `false` when nobody is there to answer a challenge.
Some policies ask for more when nobody is present.

## 3. Call tools through awzy-gateway, or check yourself

- **MCP tools:** point your agent at awzy-gateway instead of the MCP server:
  `https://<gateway>/mcp/<server>`. Every tool call is checked before it runs. See
  [Putting MCP servers behind awzy-gateway](/guides/mcp-gateway/).
- **Your own code:** ask the PDP directly before an action, over gRPC or HTTP, and apply the
  verdict. See the [decision API](/reference/decision-api/).

## 4. Handle challenges and denials

A blocked MCP tool call comes back as a tool error, with `_meta`:

| `_meta` field | Meaning |
| --- | --- |
| `awzy/verdict` | `deny`, `challenge`, or `unavailable` (awzy couldn't reach its PDP) |
| `awzy/decision_qid` | The decision's ID, for the security team to explain |
| `awzy/challenge_qid` | For a challenge: the one to answer |

What your agent should do:

- **Challenge:** tell its person what's needed and where to answer it (the client portal's
  Challenges page). Retry the **same** call once it's passed. A passed challenge holds for that
  exact request, for a short while. Don't try to work around it with a different resource or
  tool.
- **Deny:** stop, and tell its person, with the decision ID. Retrying won't change it.
- **Unavailable:** retry after a short wait.

## 5. Test in monitor mode

While the install is in monitor, nothing is blocked, but every decision is recorded. Run your agent
through its normal work and look for its would-denies on the Decisions page. Fix its permissions
or your policies before enforcement starts.

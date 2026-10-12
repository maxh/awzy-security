# 0111. An agent with no person present waits in awzy for its challenge's answer

- Status: Accepted
- Date: 2026-10-12
- Builds on: [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (SDKs surface challenges), [0073](0073-awzy-challenge-tells-people-whats-waiting-for-them.md) (people hear about challenges), [0080](0080-signed-enforcement-modes-and-grants-per-policy.md) (grants per policy)

## Context

An agent running a script gets a challenge for a write. Its person isn't in the agent's session,
so the agent can't show them a prompt. awzy already sends the challenge to that person, tells them
in Slack, and allows the same request on retry once it's passed. What's missing is a way for the
agent to know when to retry. `GetChallenge` exists, but only the client portal can call
awzy-challenge, and it answers about any challenge.

## Decision

- **The PDP answers `Authorizer/WaitForChallenge`** (and `POST /v1/access/challenge`) for the
  enforcement points that already call it. Given the challenge's QID and the principal the
  enforcement point validated, it waits up to 25 seconds for an answer and says what became of
  it: pending (with the kinds left and when it expires), passed (and until when a retry is
  allowed), rejected (and until when the request is denied), or expired.
- **Only the principal a challenge was opened for, with the same person, gets an answer.** For
  anyone else it isn't found, so an agent can't learn about other people's requests by guessing
  QIDs. The PDP may read challenges from awzy-challenge for this.
- **awzy-gateway serves the same wait to MCP agents** at `/mcp/challenges/<qid>`, signed in with
  the agent's own token, and points a challenged tool call's `_meta` there.
- **The answer stays with awzy.** Waiting hands back no grant or proof. The agent retries the same
  request, and the PDP finds the grant itself, as it would for any retry. The person answers in
  the client portal or from Slack. An agent signed in as a service account can't answer a
  challenge, whoever it acts for.

Rejected: a callback to the agent. Agents run behind NATs and in scripts with no address to call.
Rejected: a streaming watch. Long polls of 25 seconds work through every proxy, and an agent
waiting on a person is in no hurry for the extra second.

## Consequences

- An SDK's challenge result carries the challenge's QID and, from its first wait, how long it
  can still be answered. The SDK offers a wait-then-retry that apps can use as is.
- Each waiting agent asks awzy-challenge about once a second while it waits. A challenge waits on
  a person, so these are few.

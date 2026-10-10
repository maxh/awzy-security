# 0016. `Human` and `ServiceAccount` principals; delegation per request

- Status: Accepted
- Date: 2026-10-08
- Partly supersedes: 0008 (how an unattended agent's owner is treated)

## Context

The first Cedar schema had `User`, `Team`, and `Agent { controller: User }`. That model breaks in
two ways once service accounts and AI agents are real:

- A shared agent acts for different people on different requests, so "the human it acts for"
  belongs to the request, not to the agent entity.
- Some agents run with no person behind them at all (scheduled jobs, autonomous agents). Someone
  is still accountable for them, but they act on their own authority, not on that person's.

We considered:

| Option | Why not |
| --- | --- |
| `User`, `Agent`, `Workload` | Splits by "is it AI", which is not what policies care about most. A non-AI job and an unattended AI agent hold the same kind of credential and carry the same kind of risk. |
| `User`, `Agent` (delegated), `ServiceAccount` (own authority) | Better split, but an AI system that sometimes acts for a person and sometimes runs alone needs two identities. And whether a request is delegated is a property of the request, not of the identity. |
| `Human` and `Bot` | Short and symmetric, but connectors import these identities from Kubernetes, GCP, Entra, and Okta, which call them service accounts. "Bot" also suggests a chat bot, and reads informally in an audit trail. |
| `User` instead of `Human` | IdPs list service users among their "users", so `User` is ambiguous. |

## Decision

- Principals are `Human` (a person) and `ServiceAccount` (any non-human identity with its own
  credentials: a backend service, a scheduled job, or an AI agent). Both can be members of
  `Group`s.
- `ServiceAccount { owner: Human, ai: Bool }`. The owner is accountable: challenges and alerts
  for the account go to them. `ai` marks AI agents for policies that treat AI differently.
- Delegation is per request, in the Cedar context, and only the PDP sets it:
  - `on_behalf_of: Human`, optional: the person a service account is acting for, taken from a
    validated RFC 8693 token (subject = the person, `act` = the account, per 0008). Absent when
    the account acts on its own authority.
  - `human_present: Bool`: whether a person is there to answer a challenge now. Defaults to true
    for a `Human` subject and false for a service account unless the enforcement point says
    otherwise.
  - The PDP rejects a request whose free-form context sets either key, so a caller can't claim
    delegation it doesn't have.
- A delegated request gets two Cedar checks: one for the service account and one for the person,
  on the same action and resource. The stricter verdict wins (deny, then challenge, then allow).
  A service account acting for someone gets at most what both it and that person could do. The
  decision records both checks so explain-decision can show which one decided.
- An unattended service account has no `on_behalf_of`. Its owner is not treated as present or as
  the person it acts for. This replaces 0008's "the registered owner is the controlling human".
- An optional `client` context field records what the enforcement point observed about the
  calling software (`browser`, `cli`, an MCP client name). It is the first signal for an agent
  that uses a person's own credentials (below).

### Agents using a person's own credentials

When an agent runs with a person's own token, the PDP sees a `Human` and identity alone can't
tell. We handle this in three ways rather than blocking it:

1. Make declared delegation the easy path: awzy-gateway issues delegated tokens, and
   device-bound tokens stop working when copied into an agent elsewhere.
2. Detect it: the `client` field now, and later an automation score from awzy-reasoner based on
   machine-speed patterns (the paper's "suddenly foolish administrator").
3. Challenge, don't block: a `Human` who looks automated on a sensitive action gets a security
   key challenge, which an agent can't answer.

## Consequences

- One AI system can act for alice on one request and run unattended on the next under a single
  identity, and each decision records which mode it was in.
- A delegated request costs a second Cedar evaluation, about 1 to 2 ms more against the 10 ms
  budget.
- When several `@challenge` policies fire, the verdict names only one challenge kind, so a request
  that needed both approval and a security key asks only for approval. Requiring all of them is
  still to do.
- The decision log and ClickHouse table record `on_behalf_of`, `human_present`, and the person's
  verdict and policies (migration 0003).
- The CheckRequest field `controller` became `on_behalf_of`, and `human_present` was added.

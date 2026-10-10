# 0008. Agent identity through token exchange

- Status: Accepted; partly superseded by [0016](0016-human-and-serviceaccount-principals.md) (unattended agents and their owners)
- Date: 2026-10-08

## Context

Every decision is about one accessor, and an agent's decision also needs its controlling human.
Identity asserted in headers or MCP arguments can't be trusted.

## Decision

- A PEP validates a token (signature, issuer, audience, expiry) and passes only validated claims
  to the PDP. SDK PEPs validate again and never accept a bare user ID.
- An agent acting for a human uses OAuth token exchange (RFC 8693). The token's subject is the
  human and its `act` claim is the agent, plus an optional task ID.
- Unattended agents use their own workload or client-credentials token. Their registered owner is
  the controlling human and `human_present = false`.
- Every agent is registered in awzy-context with an owner. Unregistered agents are denied (or
  recorded, in monitor mode).
- Use the client IdP's token exchange or on-behalf-of flow where it exists. Otherwise awzy-gateway
  issues short-lived tokens signed with the install's KMS key.

## Consequences

- How often awzy-gateway must act as the issuer depends on design partners' IdPs (open question).

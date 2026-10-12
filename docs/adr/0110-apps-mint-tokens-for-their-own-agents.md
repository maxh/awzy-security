# 0110. Apps mint tokens for their own agents, the service accounts under their name

- Status: Accepted
- Date: 2026-10-12
- Builds on: [0008](0008-agent-identity-through-token-exchange.md) (validated tokens), [0016](0016-human-and-serviceaccount-principals.md) (`act`), [0033](0033-service-account-registry-in-awzy-context.md) (the service account registry), [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (SDKs validate tokens)

## Context

A design partner's app mints its own agents: interactive ones acting for a staff member, and
long-running ones under a service account. Their tokens come from the app, not from the client's
IdP, which is the only issuer the gateway, the portal, and the SDKs trust. And the agents come and
go with the app's own records, so registering each one by hand in the client portal doesn't keep
up.

Trusting the app as an issuer is a grant of power: whatever it signs, awzy believes. So it needs a
limit an admin sets, which the app can't widen.

## Decision

- **An admin registers an app as a token issuer** in the client portal: its name, the `iss` its
  tokens carry, its JWKS URL, and the `aud` they must carry. awzy-context keeps the list. An app's
  `iss` can't be another's.
- **An app signs tokens only for service accounts whose ID starts with its name and a dot**, its
  prefix: `acme-crm` signs for `acme-crm.renewals-bot`. Names have no dots, so no two apps'
  prefixes overlap. The service account is the token's `sub`, or its `act.sub` when
  it acts for a person (0016). A token whose principal would be a `Human` acting for themselves,
  or a service account outside the prefix, is refused like a bad signature. So an app can't speak
  for a person on its own, nor for an agent another app or the IdP owns.
- **An app's agents are still registered service accounts** (0033), each owned by a person the
  IdP provisioned. One under the prefix that isn't registered gets as far as the PDP, which
  denies it as `unregistered-service-account`, so the attempt is in the decision log.
- **The app registers and retires its agents itself**, with a token it signs for itself (`sub` is
  its name): `GET`, `PUT`, and `DELETE /api/v1/agents/<id>` on the client portal, for IDs under
  its prefix only. That token does nothing else, and an agent's token can't register agents. Each change is recorded with the app as the actor (`App::acme-crm`).
- **Every enforcement point gets the list from the PDP**, `Authorizer/ListTrustedIssuers`, which
  takes it from awzy-context with the principals snapshot. awzy-gateway and the portal refresh it
  every few seconds. An SDK, which validates tokens itself (0045, rule 1), does the same.

Rejected: a list of service accounts per app, set by the admin. Each new agent would wait on an
admin, which is what the partner can't do. A prefix keeps the limit admin-set and lets the app add
agents inside it. Rejected: a prefix the admin chooses per app. Its name is one fewer thing to set
and to check for overlaps, and it says in every agent's ID which app vouches for it. Rejected: token exchange at the IdP for every agent. It works for an agent acting
for a person who signed in, but not for long-running agents, and it puts the app's agent lifecycle
in the IdP's admin console.

## Consequences

- An app's signing key is as sensitive as the agents under its prefix. Removing the app as an
  issuer stops its tokens within the refresh interval; its agents stay registered until removed.
- An agent's owner is still a person, so a contained person's agents follow them (0033) whoever
  minted their tokens.
- Tokens from the IdP are unchanged, and checked first.

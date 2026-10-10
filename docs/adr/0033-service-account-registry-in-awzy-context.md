# 0033. Service accounts are registered in awzy-context; the PDP denies unregistered ones

- Status: Accepted
- Date: 2026-10-09
- Builds on: [0016](0016-human-and-serviceaccount-principals.md), [0031](0031-people-and-groups-from-the-idp-over-scim.md)

## Context

0016 made every service account (backend service, scheduled job, AI agent) an entity with an
accountable owner, and the plan says a token for an unregistered service account is denied.
Until now each enforcement point kept its own list in its config: the gateway's JSON, the client
portal's file. Lists drift apart, an account registered for one path is unknown on another, and
no one place answers "which agents exist and who owns them".

## Decision

- **awzy-context keeps the registry:** ID (the token's `sub`, or `act.sub` when acting for a
  person), owner, whether it is AI, groups, and a description, with who registered or changed it.
  The owner must be a person the IdP has provisioned (0031).
- **Client admins register accounts in the client portal**, which authorizes each change through
  the PDP as `administer` on the `service_accounts` portal area (0020) and records the actor.
- **Service accounts join the principals snapshot** the PDPs load, with `owner`, `ai`, and their
  groups, so every enforcement point sees the same account.
- **The PDP enforces registration.** With awzy-context configured, a check whose subject is a
  service account awzy-context doesn't know is denied without evaluating policy, and logged with
  the reason `unregistered-service-account`.
- **Enforcement points defer** with `service_account_registry: awzy-context` (gateway) or
  `AWZY_SERVICE_ACCOUNT_REGISTRY=awzy-context` (client portal): they pass the account on as a bare
  entity and the PDP fills it in or denies it. Config lists remain for installs without
  awzy-context.
- **Telling an application's own token from a person's** without a local list: a token whose
  RFC 9068 `client_id` (Okta: `cid`) equals its `sub`, or Entra's `idtyp: app`, is a service
  account acting on its own authority.

## Consequences

- One registry for every access path, and a page that lists every agent and its owner.
- An account must be registered before it can do anything, on every path, including in monitor
  mode (the deny is recorded as a would-deny there).
- The owner reference is only as current as the IdP's provisioning: deprovisioning the owner
  doesn't remove the account. Flagging orphaned accounts is future work.
- awzy-context's registry API has no authentication of its own yet; only the client portal may
  reach it, by network policy, until service-to-service authentication is decided.

# 0031. People and groups come from the IdP over SCIM; the PDP takes them from awzy-context

- Status: Accepted
- Date: 2026-10-09

## Context

Until now each enforcement point built the caller's Cedar entities from the token: the subject
and the `groups` claim. That has three problems. A token minted before a change keeps the old
groups until it expires. Deprovisioning in the IdP doesn't reach awzy until then either. And
policies can only use what fits in a token. Phase 2 starts the world model in awzy-context, and
people and their groups come first.

The plan described connectors as pull-based jobs calling each system's API. For an IdP that
means storing an admin API credential for Okta, Entra ID, or Google Workspace in the install, and
deprovisioning arrives only at the next poll.

## Decision

- **The IdP pushes people and groups to awzy-context over SCIM 2.0** (`/scim/v2`), the standard
  provisioning protocol Okta, Entra ID, and Google Workspace all speak. The client configures
  awzy as a SCIM app in their IdP with a bearer token they generate. awzy holds no IdP admin
  credential, and changes arrive as they happen. Other systems of record (HR, Jira) remain
  pull-based connectors.
- **Identity:** a person's Human ID is their SCIM `userName`, unique case-insensitively; a group's
  ID is its `displayName`, so policies say `Group::"security"` as the IdP names it. Tokens are
  matched to people by a configurable claim (`subject_claim`, default `sub`; `email` or
  `preferred_username` with Okta and Entra, whose `sub` is an opaque ID).
- **awzy-context** keeps people, groups, and memberships (nested groups included) in Postgres, and
  serves them to PDPs as Cedar entities at `/v1/principals`, with a version that moves on every
  change.
- **The PDP** loads that snapshot, polls for a newer version every 5 seconds, and isn't ready
  until the first one loads. For each check it replaces the subject's and the person's entities,
  and their group ancestry, with the snapshot's. People the snapshot doesn't know (and service
  accounts, for now) keep what the enforcement point sent.
- **Deprovisioning:** a deactivated person carries no groups, and the base policy
  `deactivated-people` denies them everything, whatever their token says.
- People and groups go in a snapshot rather than Valkey and the Kafka `attributes.v1` feed: even a
  large client has tens of thousands of them, which fits in memory and polls cheaply. Resources,
  which can number in the millions, are where Valkey and the change feed come in.

## Consequences

- A group change or deactivation in the IdP reaches every PDP within seconds of the IdP sending it,
  without waiting for tokens to expire.
- The policy bundle's schema must declare `Human.active`. Publish the release's base bundle
  (`awzy-publish`) before pointing PDPs at awzy-context, or every check fails entity validation.
- The IdP must reach `/scim/v2` from the internet (Okta and Entra call it from their cloud): the
  chart needs a route for it, with TLS, and nothing else of awzy-context exposed.
- `/v1/principals` has no authentication of its own yet, like awzy-policy's API: a network policy
  must keep it to the PDPs until service-to-service authentication is decided.
- Service accounts still come from enforcement points' config; registering them in awzy-context,
  with owners, is next.

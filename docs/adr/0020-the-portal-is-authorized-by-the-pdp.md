# 0020. The client portal signs viewers in and asks the PDP, always enforcing

- Status: Accepted
- Date: 2026-10-09

## Context

Until now the client portal and its MCP server had no sign-in. Anyone who could reach them could
publish policy and switch the install between monitor and enforce, and every change was attributed
to "client-portal". The portal controls enforcement for the whole install, so it is the most
sensitive thing awzy runs in a client cloud.

There were two ways to decide who may use it: a fixed rule in the portal's code (for example,
members of an admin group), or the same PDP and Cedar policies that cover the client's own
resources.

## Decision

- **Sign-in.** Every `/graphql` POST and `/mcp` request needs a bearer token from the client's IdP.
  awzy-identity validates it the same way awzy-gateway does (JWKS, issuer, audience; ADR 0008). An
  agent acting for a person presents a token-exchange token and is checked as that service account
  acting for them (ADR 0016). The browser signs in with OIDC authorization code and PKCE; the
  backend tells it which IdP at `/auth/config`. MCP clients find the IdP through OAuth protected
  resource metadata (RFC 9728) on a 401.
- **The PDP decides.** Each operation is a Cedar check: `view` or `administer` on a `PortalArea`
  (`decisions`, `policies`, `enforcement`), with `context.client` set to `portal` or `mcp`. The base
  policies let the `security` group do both. Clients change who runs the portal by editing policy,
  like anything else, and every portal access lands in the decision log.
- **Always enforced.** The portal applies the PDP's verdict even when the install is in monitor
  mode, and marks its checks `always_enforced` so the decision log records them as enforced.
  Monitor mode is for rolling out policy on the client's resources; it must not open the portal
  that controls it. A challenge is refused until awzy-challenge exists. If the PDP can't be reached,
  the portal refuses.
- **Attribution.** Changes are recorded under the signed-in actor: `alice`, or `sales-genie for alice`.

## Consequences

- A bad bundle can lock everyone out of the portal. Break-glass rollback recovers from that without
  the IdP, PDP, or portal.
- A bundle must contain the `PortalArea` schema and a rule granting someone access, or nobody can use
  the portal. Installs whose active bundle predates this record need a new bundle published
  directly: `awzy-publish` publishes the release's base bundle to awzy-policy.
- awzy-policy's gRPC API still has no authentication of its own: anything that can reach it inside
  the cluster can publish. Until it gets mTLS or workload identity, a network policy must keep
  everything but the portal and the operator tools away from it.
- Local development needs an IdP. `tools/dev-idp` is a development-only OIDC provider with a
  "sign in as" page and token exchange for agents. It must never be deployed.
- The corp portal still has no sign-in. It holds no client data, but it needs sign-in against
  awzy's own IdP before anyone outside development uses it.

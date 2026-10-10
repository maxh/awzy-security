# 0032. The demo install signs in with the dev IdP

- Status: Accepted
- Date: 2026-10-09
- Builds on [0027](0027-demo-and-corp-hostnames.md)

## Context

The client portal won't start without an identity provider (IdP) to trust. The example client's
install is the product demo at `demo.awzysecurity.com` (0027). People viewing the demo don't have
accounts in any IdP of the example client, and setting up a real IdP for each viewer isn't worth
it for sample data. `tools/dev-idp` already provides sign-in for local development and tests.

## Decision

- Demo installs run the dev IdP at `https://<hostname>/idp`, through the install's Gateway, which
  terminates TLS and strips the `/idp` prefix. The dev IdP serves plain HTTP there
  (`AWZY_DEV_IDP_TLS=off`); its sign-in page uses relative links so it works under the prefix.
- It is switched on by `demoIdp.enabled` in the awzy chart (`demo_idp` in `bootstrap/`), off by
  default. When on, the client portal trusts it as its issuer and reads its keys inside the cluster.
- The image is published as `awzy/awzy-dev-idp` alongside the shipped services.

## Consequences

- **Anyone who can reach the demo can sign in**, as one of the sample users, with no password.
- **The dev IdP's signing key is fixed and derivable from the source**, so anyone can mint tokens it
  would accept. That is acceptable only because the demo holds sample data. A real client's
  install must never set `demoIdp.enabled` or trust the dev IdP's issuer.
- Agent flows (client credentials, token exchange) work against the demo the same way, which is
  useful for showing the MCP proxy.

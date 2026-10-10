# 0027. `demo.awzysecurity.com` and `corp.awzysecurity.com`

- Status: Accepted
- Date: 2026-10-09
- Amends: [0018](0018-awzysecurity-domain-and-dns.md)

## Context

The client portal runs inside each client's cluster and serves only that client's data (0002,
0009), so awzy has no single hosted client app to put at a name like `app.awzysecurity.com`.
awzy still needs a public place to show the product, and the corp portal needs its name.

## Decision

- `corp.awzysecurity.com`: the corp portal, in the awzy organization (0018 reserved it).
- `demo.awzysecurity.com`: the client portal of the example client's install
  (`awzy-example-install`), as the product demo. The DNS record points at that install's load
  balancer; traffic and data stay in the example client's account.
- Real clients serve their client portal on a domain they own (for example `awzy.acme.com`).
  awzy doesn't put client portals under `awzysecurity.com` by default, because whoever controls
  the DNS name can get certificates for it.
- Records are in Cloudflare (0018). TLS certificates are issued where the load balancer runs and
  validated with a DNS record in Cloudflare.

## Consequences

- The awzy chart needs ingress (Gateway API with Envoy Gateway, per the platform plan) and the
  demo install needs an IdP for portal sign-in before `demo.awzysecurity.com` works.
- `demo.awzysecurity.com` is down whenever the example install is destroyed between test runs.

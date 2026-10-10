# 0018. `awzysecurity.com` for account email and vendor endpoints; DNS on Cloudflare

- Status: Accepted, amended by [0027](0027-demo-and-corp-hostnames.md)
- Date: 2026-10-09
- Edited for publication: account identifiers removed; deploy/terraform/*/deployment.env holds them.
- Amends: [0014](0014-two-aws-organizations.md) (root emails), [0017](0017-test-installs-on-gcp-early.md) (no domain yet)

## Context

awzy now owns `awzysecurity.com`, registered with Cloudflare Registrar. Until now the AWS root
emails were going to be Gmail plus-addresses, and 0017 assumed no domain, so the GCP example
client had no organization. Root email is how an AWS account is recovered, so it should live on a
domain the company controls and can hand over. Cloudflare Registrar requires Cloudflare DNS.

## Decision

- **DNS stays on Cloudflare** for the apex. Records are managed by Terraform with the Cloudflare
  provider in `deploy/terraform/corp/dns/`, once that tree exists. Subzones that belong to an AWS
  account (for example `example-client.awzysecurity.com`) are delegated to Route 53 with NS records.
- **Lock the domain down first.** MFA on the Cloudflare account, registrar lock and auto-renew on,
  and DNSSEC enabled. The domain now controls account recovery for every awzy cloud account.
- **Account email uses the domain**, forwarded to the founder's inbox by Cloudflare Email Routing,
  with no mailbox service:
  - One alias per account, `aws-<account>@` the domain, each the root email of its account.
  - Member accounts created later get their own alias the same way.
- **Vendor endpoints get stable names** under the domain, so the service behind them can change
  without touching installs: `registry.awzysecurity.com` (the vendor registry; see the open
  registry question in 0017), `health.awzysecurity.com` (health intake), and
  `corp.awzysecurity.com` (corp portal).
- **Google Cloud:** Cloud Identity Free on `awzysecurity.com` gives awzy a Google Cloud
  Organization. That organization is awzy's, not the example client's.

## Consequences

- The GCP example client still needs an identity of its own to get a separate organization: a
  second inexpensive domain, or a subdomain of `awzysecurity.com` as the primary domain of a second
  Cloud Identity account (not yet verified that Google accepts this). Decided before the GCP
  install.
- Cloud Identity on `awzysecurity.com` can later be awzy's staff IdP, for the corp portal (OIDC)
  and as the external IdP for AWS IAM Identity Center. Not decided here.
- Losing the Cloudflare account means losing account recovery for every awzy cloud account, so its
  recovery codes are stored with the AWS root credentials.

# 0014. Two AWS organizations for development

- Status: Accepted (accounts not created yet), amended by [0018](0018-awzysecurity-domain-and-dns.md) and [0023](0023-aws-organization-as-built.md)
- Date: 2026-10-08

## Context

awzy needs a vendor environment and a realistic client install to develop and test against. A
client account is never in awzy's organization (0002, 0009), so a test install inside the vendor
organization would hide every cross-boundary problem.

## Decision

- Two standalone AWS accounts, each the management account of its own organization:
  `awzy-corp` (vendor) and `awzy-example-client` (plays a client). Neither is the developer's
  personal account.
- Each has root MFA, no root access keys, and IAM Identity Center in us-east-2. The CLI profiles
  are `awzy-corp` and `awzy-example-client`.
- Workloads go in member accounts, not the management accounts: `awzy-build`, `awzy-staging`, and
  `awzy-corp-portal` on the vendor side, and `awzy-example-install` on the client side.
- The example client install crosses the boundary only the way a real client would: it pulls
  signed images and pushes health metrics.
- Scale and performance tests run against the example install and are monitored in that account
  (Amazon Managed Service for Prometheus, Amazon Managed Grafana, X-Ray, CloudWatch, k6 as
  Kubernetes Jobs). The corp portal sees only the health export, as it would for a real client.
- Terraform lives in `deploy/terraform/corp/` (vendor accounts) and `deploy/terraform/aws/`
  (the client module), with `examples/example-client` instantiating the client module.

Deployment plan: [../plans/deployment-plan.md](../plans/deployment-plan.md)

## Consequences

- Each account gets a budget alert as soon as it exists.
- The target load for performance tests is not defined yet.

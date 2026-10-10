# 0023. The awzy AWS organization as AWS actually built it

- Status: Accepted
- Date: 2026-10-09
- Amends: [0014](0014-two-aws-organizations.md)
- Edited for publication: account identifiers removed; deploy/terraform/*/deployment.env holds them.

## Context

0014 assumed a classic sign-up: a standalone account that becomes an organization's management
account, with IAM Identity Center enabled in us-east-2 and `aws configure sso` profiles. The
`awzy-corp` sign-up instead landed in AWS's new sign-up experience (projects and spend limits).
That experience doesn't support AWS Organizations, IAM Identity Center, or Amazon Managed Grafana
until "advanced features" are activated, which can't be undone.

## Decision

- **Activate advanced features** on the awzy side (done 2026-10-09). The example client does the
  same, or uses "Sign up for AWS (advanced)" if the sign-up page offers it.
- **Accept the structure AWS created** rather than rebuild it:
  - The organization and its management account, under a root email alias on the domain
    (0018). No workloads in it.
  - An AWS-created member account, "Identity Delegated Admin", administers the IAM
    Identity Center instance `AWS-Awzy-Security-Team`, from us-east-1. Identity source stays AWS
    Builder ID for now.
  - The former project account is reused as `awzy-build`.
- **CLI access uses `aws login`** (console session to temporary credentials), not
  `aws configure sso`: `aws login --profile awzy-corp --region us-east-2`. Terraform runs with
  `aws configure export-credentials --format env` so any provider version works.
- **Member accounts are Terraform** in `deploy/terraform/corp/org/`, applied with OpenTofu. Its
  state, and all `corp/` state, is in an S3 bucket in the management account (created by `corp/state/`), not in `awzy-build` as 0014's table said,
  because the organization layer has to run as the management account anyway.
- **Keep the AWS-managed policies for now**, and change them deliberately in Terraform later:
  - SCP `AdvancedModeRegionRestrictionSecurityControlPolicy`: only us-east-1, us-east-2, and
    us-west-2. us-east-2 is the working Region.
  - SCP `ManagedAccountSecurityControlPolicy`: protects `role/managed/*` roles and blocks
    leaving the organization.
  - RCP `ManagedAccountResourceControlPolicy`: denies principals outside the organization access
    to `ecr`, `s3`, `kms`, `sts`, and other services. Cross-organization image pulls by the example
    client (deployment plan step 4) need an exception for `ecr` scoped to the example client's
    organization ID, or the registry decision in 0017 must avoid ECR.
  - Five detached `BudgetsSpendLimitDeny*` SCPs left from the spend limit. Delete them.
- **Spend control** is the AWS Budget `awzy-org-monthly` ($50/month, alerts at 80% actual and
  100% forecast to the management account's email), since activation removed spend limits.

## Consequences

- New organizations start with a "Maximum number of accounts" quota of 2 (default 10). An
  increase to 10 was requested on 2026-10-09; `awzy-staging` and `awzy-corp-portal` are created by
  `corp/org` once it is approved. The example client's organization will hit the same limit.
- `aws login` sessions follow the console session, so they expire with it. Moving to Identity
  Center users and `aws configure sso` can come later, together with choosing a staff IdP (0018).
- Identity Center is administered in us-east-1 even though workloads are in us-east-2. That
  doesn't affect installs.

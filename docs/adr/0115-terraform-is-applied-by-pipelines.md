# 0115. Terraform is applied by pipelines, the corp accounts' and the example clients'

- Status: Accepted
- Date: 2026-10-12
- Amends: [0014](0014-two-aws-organizations.md) (nothing in CI reaches the example client)
- Builds on: [0035](0035-vendor-services-on-ecs-deployed-by-ci.md) (CI deploys the vendor services)

## Context

Code already ships through pipelines: CI builds, signs, and publishes every image and the chart,
deploys the corp portal's services (0035), and each install's Argo CD follows its release channel.
Terraform doesn't. A person applies it from a laptop, with credentials from `aws login` and inputs
from a gitignored `deployment.env`, for the corp accounts and for both example clients. The
deployment plan chose that on purpose, so that CI's roles couldn't change an account, and for the
example client so that nothing in awzy's CI could reach a client's cloud.

It no longer holds up. A merged change waits until someone applies it, from whatever their
laptop has checked out; nothing reviews the plan that ran; and the `terraform-drift` workflow
exists only to notice when nobody did. Turning on one service in the demo (awzy-intake) became a
laptop apply against a cloud a client would treat as production.

A separate repository for the example clients, playing a client's own infrastructure repository,
was considered and set aside: it would keep awzy's CI out of their accounts and test upgrades the
way a client meets them, but split every change to a module and its use across two repositories.

## Decision

- **Terraform is planned on every pull request and applied on merge**, by the `terraform`
  workflow, for each deployment a change touches: the build account's corp modules (`corp/build`,
  `corp/corp-portal`), and each example client's modules in its install account.
- **A pull request gets the list of changes as a comment**, from a read-only plan role that pull
  requests and `main` can assume. Pull requests from forks get no token.
- **The apply runs after merge in a GitHub environment of the deployment's own**
  (`corp-terraform`, `example-aws`, `example-gcp`), which waits for a person's approval, as a role
  only that environment can assume. Each example client's roles live in its own accounts, so the
  example client still grants awzy's CI what a client would, explicitly and per account.
- **What runs in a management account stays with a person**: `corp/state`, `corp/org`, and each
  example client's `state` and `org`. They change a few times a year, and an apply role there could
  create and remove accounts.
- **Inputs move from laptop files to CI**: each deployment's `deployment.env` is an environment
  secret, as the drift check already does for corp's.
- **The roles that pipelines assume are applied by hand**, once, since a pipeline can't create the
  role it runs as. Each module's README says which.

## Consequences

- Nothing merged waits on a laptop, and every apply ran a plan someone could read first.
- An apply role can change anything in its account, including CI's other roles there. The
  environment's required approval and its OIDC subject, which no other workflow or branch has, are
  what keep it from being CI's everyday power.
- Plans stay out of public logs: pull request comments and job output list resource addresses
  only, as the drift check's do, since plans hold account IDs.
- awzy's CI can now reach the example client's install account, which 0014 ruled out. The example
  client grants it there, per environment, the way a client that let a vendor's pipeline manage an
  account would.
- The examples keep using the modules at the commit being merged, not a pinned release, so they
  don't test a client's upgrade path; the release channels and Argo CD still test the chart's.

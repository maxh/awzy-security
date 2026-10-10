# 0053. Configuration names start with AWZY_, the vendor's with AWZY_CORP_; two releases; lists are sorted

- Status: Accepted
- Date: 2026-10-09

## Context

To open-source awzy, everything specific to one deployment became configuration: accounts,
region, registry, roles, domain. That added GitHub Actions variables and Terraform inputs next
to the environment variables the services already read. The new CI variables started out as
`AWS_REGION` and `RELEASE_REGISTRY`, names that collide with other tools or say nothing about
whose they are. Meanwhile the services had settled on a pattern no record states:
`AWZY_CLICKHOUSE_URL`, `AWZY_PDP_ADDR`, `AWZY_TEST_POSTGRES_URL`, and `AWZY_CORP_SESSION_SECRET`
for the corp portal.

## Decision

**Names**

- Everything awzy reads from its environment starts with `AWZY_`, then the component or
  dependency it's about, then the setting: `AWZY_CLICKHOUSE_URL`, `AWZY_AUTH_ISSUER`,
  `AWZY_PDP_ADDR`. Settings every service shares have no component: `AWZY_LISTEN_ADDR`,
  `AWZY_LOG_FORMAT`.
- `AWZY_CORP_` marks the **vendor side** as a whole, what `deploy/terraform/corp` manages: the
  organization, the build account's registry, signing key, and CI roles, and the vendor
  deployment's GitHub Actions variables and secrets (`AWZY_CORP_AWS_REGION`,
  `AWZY_CORP_RELEASE_REGISTRY`, `AWZY_CORP_AWS_PUBLISH_ROLE_ARN`, …). A client install never
  reads an `AWZY_CORP_` name, which keeps the line 0009 draws visible in configuration too.
- **Corp is more than the corp portal.** `AWZY_CORP_PORTAL_` is the narrower group: one area of
  the vendor side, `portals/corp/`. Settings only the corp portal service reads belong there, as
  the registry proxy's belong under `AWZY_REGISTRY_`.
- `AWZY_TEST_` marks inputs only tests read. A test whose input is unset skips.
- **Two releases, named for where they live in the repo.** The **authz** release is everything
  installed in a client's cloud: `authz/`, the client portal, the demo's dev IdP, and the Helm
  chart. The **corp portal** release is the vendor's own services in `portals/corp/`: the corp
  portal and the registry proxy. Their settings are `AWZY_AUTHZ_RELEASE_*` and
  `AWZY_CORP_PORTAL_RELEASE_*` (`_REGISTRY`, `_TAG`, `_COSIGN_KEY`). Today both come from the
  same commit, with the same tag, in the same registry; separate names and publish steps let them
  get their own versions and channels later without another rename.
- A script names a value for what it is to the script, not where CI got it:
  `tools/ci/publish-images.sh authz` reads `AWZY_AUTHZ_RELEASE_REGISTRY`, which CI fills from
  `AWZY_CORP_RELEASE_REGISTRY`, the vendor deployment's registry. A bare `AWZY_REGISTRY` would
  read as the registry proxy's own setting (`AWZY_REGISTRY_UPSTREAM`).
- Settings only CI's workflow reads start with `AWZY_CI_` (`AWZY_CI_PUBLISH`).
- **Exceptions:** names another tool defines keep that tool's name: `AWS_REGION` (read by the AWS
  CLI and SDKs), `TF_VAR_*` and `TF_CLI_ARGS_*` (OpenTofu), `RUST_LOG`. Where we choose the
  value, it comes from an `AWZY_` name (`AWS_REGION: ${{ vars.AWZY_CORP_AWS_REGION }}`).
- Terraform variables are lowercase snake_case with no prefix (`build_account_id`): a module
  already scopes them, and their environment form is `TF_VAR_build_account_id`.

**Order**

- Lists of keys are sorted alphabetically wherever order doesn't matter: CI `env:` blocks,
  `deployment.env` files and their examples, `variables.tf`, and lists of names in docs. A
  sorted list is quick to scan, and two branches adding names conflict less.
- Where order matters it wins: `TF_CLI_ARGS_init` comes after the variables it expands, and
  steps run in the order they must.

Rejected: a `CORP_` prefix without `AWZY_`. Everything awzy owns should be findable with one
search.

## Consequences

- The corp portal service still reads `AWZY_CORP_ACCOUNTS_FILE`, `AWZY_CORP_SESSION_SECRET`, and
  `AWZY_CORP_INSECURE_COOKIES`. They move to `AWZY_CORP_PORTAL_` in their own change, since it
  needs the corp portal's ECS task definitions applied.

- The CI secret for the remote cache is `AWZY_CORP_BUILDBUDDY_API_KEY` (it was
  `BUILDBUDDY_API_KEY` before the public repository started).
- New settings need a moment's thought about which group they belong to. In return, a
  `grep AWZY_` finds every knob, and `AWZY_CORP_` shows at a glance what a client never sees.

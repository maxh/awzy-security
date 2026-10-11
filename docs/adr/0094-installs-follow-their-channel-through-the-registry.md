# 0094. Installs follow their release channel through the registry

- Status: Accepted
- Date: 2026-10-11
- Amends: [0015](0015-cloud-agnostic-install.md) (how an install follows a channel)

## Context

0015 planned `stable` and `fast` as small artifacts that each name a chart version, moved by the
release process, and left open how Argo CD would follow one. Nothing followed a channel yet: an
install's Argo CD Application named the exact version bootstrap was given, every merge to `main`
published a release, and nothing soaked before reaching clients. The corp portal recorded each
install's channel, and nothing read it.

ECR's tags in the build account are immutable, so a channel can't be a tag that moves. Every
install already pulls through the registry proxy (0028), which knows which install is asking.

## Decision

- **The corp portal keeps each channel's version**, as an append-only history
  (`release_channel_changes`): every publish moves `fast` to that commit's chart, and a promotion
  moves `stable`. `awzy-corp-portal channel set` makes the change, run by CI as a one-off ECS task
  like the migration (0087).
- **The registry proxy shows each install only its version.** For the chart's repository, it lists
  only the version the install's channel names (or the version it's pinned to), and refuses a
  manifest by any other tag. Digests, signatures, attestations, and images pass through as before.
- **Argo CD follows a semver range** (`>=0.0.0-0`, bootstrap's default), so it resolves to the
  one version the proxy lists, and moves when the channel does. An exact `chart_version` still
  holds an install where it is.
- **Promotion takes a soak.** `.github/workflows/promote.yml`, run from `main`, calls
  `tools/ci/promote.sh`, which refuses a version published less than a day ago, so it has run on
  `fast` first.
- **Not yet: a second person's approval.** awzy has one engineer, so a required reviewer would
  only block them. When there's a second, the promote job gets a GitHub environment with required
  reviewers and self-review prevented, and the deploy role trusts that environment.

## Consequences

- Moving a channel is a row in the corp portal; installs follow within Argo CD's next check, with
  no change to their Terraform. The Releases page shows each channel's version.
- An install can't pull a chart version its channel doesn't name, even by asking for it.
- If Argo CD or Helm stopped resolving ranges against an OCI registry's tag list, installs would
  stay where they are until a channel is pinned by version; the example install checks it.
- A staging install that follows `fast` ahead of clients is still to come.

# 0096. A release waits for the bootstrap it needs

- Status: Accepted
- Date: 2026-10-11
- Amends: [0094](0094-installs-follow-their-channel-through-the-registry.md) (what a channel can move an install to)

## Context

Since 0094, an install's chart follows its release channel without the client doing anything, but
bootstrap and the cloud layer change only when the client applies them. Some releases need them to
change first. The first was #157: its chart signs policy with a KMS key the cloud layer makes, and
its bootstrap stops making the Secret earlier charts read. A channel-following install that got
that chart before applying the new Terraform would have an awzy-policy that can't sign, and one
that applied the Terraform early would delete what its current chart needs (#171).

The options were: releases check what the install provides and wait when it's too old; channels
refuse to move past such a release until awzy marks clients ready; or every such change keeps the
old and new forms for a release. Holding a channel holds back every client for the slowest one, and
keeping both forms relies on every change getting it right.

## Decision

- **Bootstrap has a level**, `bootstrap_level` in `deploy/terraform/bootstrap`, one number for what
  it and the install contract provide. It passes the level to the chart as `bootstrap.level`.
- **The chart names the level it needs**, `bootstrap.required` in its values. When the install's
  level is lower, the chart fails to render with a message naming the awzy commit to apply. Argo CD
  renders before it applies anything, so the install keeps running its current release, and the
  Application shows the message as a `ComparisonError`.
- **A change that makes the chart need something new from bootstrap or the cloud layer raises
  both**, in the same PR. CI (`tools/ci/check-bootstrap-level.sh`) fails when they differ.
- **Level 1 is where things stand after #157.** A bootstrap from before this has no level, which the
  chart treats as 0, so every install applies bootstrap once to adopt it.
- **Keeping old and new forms for a release** (#171's third option) is still worth doing when it's
  cheap, so fewer releases have to wait at all.

## Consequences

- No release breaks an install by arriving ahead of its Terraform; the install waits, and says why.
- An install that falls behind stops taking releases, security fixes included, until the client
  applies Terraform. The heartbeat doesn't report the level yet, so awzy sees a waiting install
  only as one not on its channel's version.
- A level is coarse: any raise makes every install below it wait, whether or not the specific
  change affects it.

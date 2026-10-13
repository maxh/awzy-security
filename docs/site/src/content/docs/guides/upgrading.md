---
title: Upgrading awzy
description: What happens during an upgrade, why decisions keep flowing, and what your cluster needs for that.
---

awzy sits in front of every access it decides, so an upgrade must never be an outage. Upgrades
follow the release channel your install was registered with, or the `chart_version` you pin (see
[Installing awzy](/guides/install/)). While one runs, checks keep being decided and nobody is
signed out.

## What happens during an upgrade

Argo CD applies the new chart, and Kubernetes replaces each service's pods one at a time:

1. **A new pod starts beside the old ones**, and gets no traffic until it's ready. A PDP isn't
   ready until it has loaded your signed policy bundle and the enforcement mode, so a new PDP
   never decides with empty policy.
2. **Then an old pod is told to stop.** It reports itself not ready at once, so traffic moves to
   the others, but keeps serving for ten seconds while that happens. Then it finishes the
   requests it's working on and closes its connections cleanly, so enforcement points and agents
   reconnect elsewhere instead of failing.
3. **Repeat** until every pod runs the new version.

Before any pod is replaced, a Job for each service applies its database migrations. If one
fails, the upgrade stops there, with the old version still serving, and Argo CD shows the failed
Job. A service checks on start that the schema has what it needs. During the rollout the old
version runs against the new schema, so a release only ever adds to a schema:
anything removed was stopped being used a release earlier. That's also why you can roll back an
upgrade by pinning the previous `chart_version`: the version you return to still works with the
schema it finds.

## What your cluster needs

- **Room for one extra pod per service** while it rolls out.
- **At least two nodes, ideally in two zones.** The PDP, awzy-policy, awzy-challenge,
  awzy-context, and the client portal run two replicas, spread across nodes and zones, and a
  node drain (a cluster upgrade, a node replaced) takes one replica at a time.
- **With one replica of a service** (`replicas: 1` in your values, to save on a small install),
  upgrades stay seamless, but draining the node it runs on interrupts it briefly.

## If the PDP is briefly unreachable anyway

Enforcement points are built for it. awzy-gateway replays a recent allow for tools whose tier you
set to fail open in its configuration, and denies the rest, never replaying an allow for someone
contained before the outage. Confidential and crown-jewel tools always fail closed.

## Rolling back

When a release misbehaves, return to the one before it, then find out why. Rolling back is a pin:

1. **Ask awzy to pin your install** to the previous version in the corp portal, so the registry
   serves it again.
2. **Set `chart_version`** to that version in bootstrap, and apply bootstrap. Argo CD rolls the
   services back the same way it rolled them forward, one pod at a time, so decisions keep
   flowing.
3. **Check it** as below, with `/version` reporting the version you returned to.
4. **Leave the pin** until a fixed release is out, then unset `chart_version` to follow your
   channel again.

A version you can't pin to shows in Argo CD as `ComparisonError … not found`.

### Across a database migration

Rolling back one release is always safe, migrations included. Releases only add to a schema, and
a service starts against a newer schema than it knows, so the version you return to works with
the schema the newer one left. Nothing is migrated back, and nothing needs to be.

Something is removed from a schema only two releases after awzy stops using it. So going back
further than one release can reach a version that expects something that's since been removed. It
fails to start, its log names what's missing, and the release before it is still running. Roll
back one release at a time, and ask awzy support before going back more than one.

If an upgrade's migration Job itself failed, there's nothing to roll back: the upgrade stopped
before any service was replaced. Its log (`kubectl -n awzy-system logs job/<service>-migrate`)
says why; tell awzy support, and pin the version you're on until a fixed release is out.

### A release that needs a newer bootstrap

A release can need a newer bootstrap than your install has applied. Until you apply it, Argo CD
installs nothing and keeps the release you have running (`AwzyReleaseNotSynced`). Rolling back
never needs this: an older release works with a bootstrap newer than it asks for.

## Checking an upgrade

- `kubectl -n awzy-ops get application awzy` is `Synced` and `Healthy`.
- Every service's `/version` reports the new release, and so does your install's page in the corp
  portal.
- On your monitoring, `awzy_ready` stays at 1 for at least one replica of every service, and
  `awzy_http_request_duration_seconds` shows no rise in 5xx responses.

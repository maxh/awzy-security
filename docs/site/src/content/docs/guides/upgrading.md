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

## Checking an upgrade

- `kubectl -n awzy-ops get application awzy` is `Synced` and `Healthy`.
- Every service's `/version` reports the new release, and so does your install's page in the corp
  portal.
- On your monitoring, `awzy_ready` stays at 1 for at least one replica of every service, and
  `awzy_http_request_duration_seconds` shows no rise in 5xx responses.

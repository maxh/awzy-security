---
title: Troubleshooting
description: What to check when the client portal's Status page says a service isn't ready.
---

The client portal's **Status** page shows whether each of awzy's services is ready, and links
each one to its section here. A service the page can't reach is usually restarting; one that
answers "not ready" is up but waiting on something it needs before it can serve.

The commands below assume the chart's default namespaces: `awzy-system` for the services,
`awzy-data` for ClickHouse, and `awzy-portal` for the portal.

## Start here

```sh
kubectl -n awzy-system get pods
kubectl -n awzy-system describe pod <pod>      # why it's restarting, or why it isn't scheduled
kubectl -n awzy-system logs <pod> --previous   # what the last crash said
```

- **A service that crashes at start** most often can't reach Postgres, or finds a schema older than
  it needs. Each release migrates the databases in a Job before the services roll, named
  `<service>-migrate`. Check that it ran:
  `kubectl -n awzy-system get jobs` and `kubectl -n awzy-system logs job/<service>-migrate`.
- **One service not ready usually holds up another.** The PDP waits on awzy-policy, awzy-context,
  and awzy-challenge, so fix those first.

## awzy-pdp

Not ready until it has loaded the published policy from awzy-policy, the directory from
awzy-context, and the containment list from awzy-challenge. While it's not ready, enforcement
points get "unavailable" and fall back to their outage behavior. If those three are ready and the
PDP stays not ready, look in its logs for errors reaching them.

## awzy-policy

Ready once it has started. If it's unreachable, it's restarting: its logs say whether it couldn't
reach Postgres or found an old schema. Without it the portal can't show or publish policies, and a
PDP that restarts can't load them.

## awzy-context

Ready once it has started, and restarts for the same reasons as awzy-policy. Without it the portal's
Directory page is empty, SCIM sync from your IdP fails until it's back, and a PDP that restarts
can't load the directory.

## awzy-challenge

Not ready until it has connected to Postgres and loaded the containment list, so a PDP never starts
with an empty one by mistake. Without it, challenges and containments wait, and the portal's
Challenges page can't load.

## ClickHouse

The portal reads decisions and activity from ClickHouse. "Not configured" means your install runs
without it. "Unreachable" means the portal couldn't run a query:

```sh
kubectl -n awzy-data get pods
kubectl -n awzy-data logs <clickhouse-pod>
```

Decisions keep flowing while ClickHouse is down, and nothing is lost: they wait in Kafka and are
written when it's back.

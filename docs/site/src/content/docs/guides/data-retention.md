---
title: Data retention and backups
description: Where an install keeps its data, how long it keeps it, and what's backed up.
---

Everything awzy stores stays in your account: in your Postgres, your Kafka, your ClickHouse, and
your bucket. This page says what each one holds, how long it's kept, and what you can restore.

## Where the data lives

| Store | Holds | Kept |
| --- | --- | --- |
| ClickHouse | Every decision, and the events awzy-intake receives: the audit trail, risk scores' inputs, and the portal's Decisions page | Until `clickhouse_retention_days`, or forever (the default) |
| Kafka | Decisions and events on their way to ClickHouse | 7 days |
| Postgres | awzy's own records (below) | For as long as the install runs |
| Valkey | Nothing yet: the cloud layer makes it for the PDP's caches, which run in memory today | — |

Postgres is one cluster with a database per service:

- **awzy-context:** people and groups from your IdP, service accounts, resources and their
  classification, risk scores, and investigations.
- **awzy-policy:** policy bundles, enforcement modes, and the tool tiers that apply during an
  outage, each signed.
- **awzy-challenge:** challenges and their answers, containments, enrolled security keys,
  notifications, and support sessions.

## How long decisions and events are kept

ClickHouse keeps decisions and events until they're older than `clickhouse_retention_days`, a
bootstrap setting. The default, `0`, keeps them all. Changing it takes effect on the next sync, and
shortening it removes older rows as ClickHouse merges its data, not all at once.

Set it to the longest period your audit requirements name: once a decision expires, the portal
can't show or explain it.

Kafka keeps each topic for 7 days. That's how long ClickHouse can be down and still catch up on
what it missed.

## Backups

**Postgres** is backed up by your cloud's managed Postgres: the cloud layer keeps automated
backups for 7 days, and you can restore to any point in that window. With
`deletion_protection = true` (the default), destroying the install also takes a final snapshot.
On AWS, that's Aurora, restored from the RDS console or API.

**ClickHouse** backs up decisions and events to the install's bucket every day at 03:17 UTC, with
ClickHouse's own `BACKUP` and its own workload identity, so no credentials are stored. The first
backup of each week (ISO weeks, UTC) is full; the rest that week hold only what changed since it.
The bucket expires them after `backup_retention_days` (35 by default, a cloud layer setting), so
you can always restore from at least the last four weeks. The `awzy-clickhouse-backup` CronJob in
`awzy-data` runs them; `kubectl -n awzy-data get jobs` shows the recent runs.

**Your Terraform state and `deployment.env`** are yours to back up. The example client keeps its
state in a versioned bucket and its `deployment.env` in its own secret store.

## Restoring

- **Postgres:** restore the database to a point in time as a new cluster (on AWS, an Aurora
  point-in-time restore), point the install's Postgres settings at it, and apply the cloud layer
  and bootstrap. Each service checks at start that the database has every migration its release
  needs.
- **ClickHouse:** run
  [`deploy/runbooks/clickhouse-restore.sh`](https://github.com/maxh/awzy-security/blob/main/deploy/runbooks/clickhouse-restore.sh)
  from a checkout of the repository, with kubectl access to the install.
  - `--yes` restores the newest backup into the live tables, then rewinds ClickHouse's Kafka
    consumers to 10 minutes before it, so Kafka refills everything since. Nothing is lost that
    Kafka still holds, and nothing appears twice. Use it when ClickHouse has lost its data.
  - `--drill` does the same into a database of its own, with consumers of its own, checks that
    every live row came back exactly once, and removes it. The live install isn't touched; run
    it to practise.
  - Kafka keeps 7 days, so a backup older than that leaves a gap between it and the oldest
    message Kafka still has.
  - Rows you deleted from ClickHouse after the backup come back if Kafka still holds them, since
    Kafka refills what it has.

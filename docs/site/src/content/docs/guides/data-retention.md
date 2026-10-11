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

**ClickHouse** isn't backed up yet. Its volume is a persistent disk (EBS on AWS), so restarts and
upgrades keep the data, but losing the volume loses the decisions and events older than Kafka's 7
days. Backups to the install's bucket are planned.

**Your Terraform state and `deployment.env`** are yours to back up. The example client keeps its
state in a versioned bucket and its `deployment.env` in its own secret store.

## Restoring

- **Postgres:** restore the database to a point in time as a new cluster (on AWS, an Aurora
  point-in-time restore), point the install's Postgres settings at it, and apply the cloud layer
  and bootstrap. Each service checks at start that the database has every migration its release
  needs.
- **ClickHouse:** there's no restore yet. If its volume is lost, a new ClickHouse starts empty and
  takes new decisions and events from there.

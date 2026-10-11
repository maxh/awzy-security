# 0104. ClickHouse backs up with its own BACKUP to the install's bucket

- Status: Accepted
- Date: 2026-10-11

## Context

ClickHouse holds every decision and event: the audit trail, and the inputs to risk scores. Kafka
holds the last 7 days of both (0006). Nothing backed ClickHouse up, so losing its volume lost
everything older than that. The cloud layer already made a bucket for "the ClickHouse cold tier,
and backups" and gave ClickHouse's workload identity access to its `clickhouse/` prefix.

#115 planned `clickhouse-backup`, a sidecar with its own image, configuration, and credentials.
ClickHouse 25.8 has `BACKUP` and `RESTORE` built in, writing to S3-compatible storage with
the server's own credentials: on AWS, the Pod Identity role the cloud layer already binds, so no
key is stored anywhere. On the example install it backed up, incrementally, and restored through
that role with nothing else installed.

A test also showed that "losing ClickHouse loses nothing" (0006) needed a qualification. The
consumer group's offsets live in Kafka, so a new ClickHouse with the same group resumes where the
old one stopped, and refills nothing. A new group starts from the oldest message instead, and
refills deleted rows too.

## Decision

- **ClickHouse's own `BACKUP`, to the install's bucket.** A CronJob in the chart
  (`awzy-clickhouse-backup`, daily) asks ClickHouse to back up `decisions` and `events`; ClickHouse
  writes the backup itself. The Job holds only the ClickHouse password. The chart takes an
  S3-compatible URL; bootstrap passes the cloud layer's `object_store.url`. Without one, there's
  no CronJob.
- **A full backup each week, and incrementals on it.** Each run takes a full backup if its ISO
  week (UTC) has none, and otherwise only what changed since that week's full. A restore needs at
  most two backups.
- **The bucket expires backups** (`backup_retention_days`, 35 by default, at least 14), so awzy
  never deletes them itself, and a restore point is always available from the last
  `backup_retention_days` less a week.
- **Only the data tables.** The Kafka queue tables and their views aren't backed up: the migrate
  Job makes them, and restoring them could start a second consumer.
- **A restore rewinds the consumer group.** `deploy/runbooks/clickhouse-restore.sh` restores the
  newest backup, deletes from it what came after 10 minutes before it, rewinds the group to that
  time with Kafka's own tool, and starts the consumers: what Kafka still holds comes back once,
  and nothing twice. `--drill` does the same into its own database and group, and checks every
  live row came back exactly once.

## Consequences

- Backups need no image, sidecar, or stored credential beyond what an install already has. On
  another cloud, the cloud layer supplies an S3-compatible URL ClickHouse's identity can write;
  Azure Blob Storage would need the chart to use ClickHouse's `AzureBlobStorage` destination
  instead.
- A backup older than Kafka's 7 days leaves a gap a restore can't fill. Daily backups keep it to
  a day at worst.
- Rows deleted from ClickHouse after a backup come back on restore if Kafka still holds them.
- A failed backup shows only as a failed Job; no alert covers it yet.
- The runbook needs kubectl access and starts a short-lived pod with Apache Kafka's image, in a
  namespace whose images Kyverno doesn't check.

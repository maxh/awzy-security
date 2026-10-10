# 0042. awzy-reporter reads ClickHouse's disk and consumer positions, and Kafka's offsets

- Status: Accepted
- Date: 2026-10-09
- Amends: [0026](0026-install-registry-and-heartbeats.md)

## Context

The heartbeat schema (0026) has two figures awzy-reporter has never filled in: Kafka lag and
ClickHouse disk use. They are what fails first under load, and the corp portal's charts show
them as "not reported". 0026 gave the reporter read access to the awzy namespaces' Deployments
and Pods and nothing else, so measuring them changes what it can reach inside the install.

ClickHouse is the only consumer of awzy's topics (the Kafka engine tables for decisions and
events), and it tracks its own position in each partition. Kafka knows the latest offset. Lag is
the difference.

## Decision

- **ClickHouse disk use** is the most-used disk's share in `system.disks`
  (`total_space - free_space`).
- **Kafka lag** is, summed over every partition of every ClickHouse Kafka consumer, the
  partition's latest offset minus the consumer's position. Positions come from
  `system.kafka_consumers`; a partition the consumer hasn't read yet counts from the earliest
  offset. Latest and earliest offsets come from Kafka's ListOffsets. The topics are the ones the
  consumers are assigned, so the reporter is told nothing about topics.
- **A ClickHouse user of its own, `awzy_reporter`**, `readonly = 1`, granted exactly:
  `SELECT(name, free_space, total_space) ON system.disks`, `SELECT ON system.kafka_consumers`,
  and `SHOW ON *.*`. ClickHouse lists a Kafka table's consumers only to users who can see that
  table, and `SHOW` is the narrowest grant that does: table and column names, no rows. With
  `select_from_system_db_requires_grant` on (the chart sets it rather than rely on the default),
  the user can't read `system.query_log`, any other system table, or any awzy table. Checked on
  ClickHouse 25.3.
- The `awzy-clickhouse-migrate` Job creates the user and sets its password on every sync, from
  a password bootstrap generates (`awzy-clickhouse-reporter`, in the data and system
  namespaces). If ClickHouse loses its volume, the next sync recreates it.
- **Kafka access is metadata and offsets only**: the reporter never fetches records and never
  joins a consumer group.
- Either figure is left out of the heartbeat when its source can't be reached, as before.

Rejected: reading the consumer group's committed offsets from Kafka (OffsetFetch). The Rust
Kafka client awzy uses (rskafka) has no consumer-group APIs, and ClickHouse's own view is the
position that matters. Rejected: a general read-only ClickHouse user. System tables such as
`query_log` hold query text, which can carry client data.

## Consequences

- The reporter holds a ClickHouse credential and can reach Kafka. Neither lets it read decisions,
  events, or query text, and it still sends only the fixed heartbeat schema (0009).
- `SHOW ON *.*` lets it list awzy's table and column names: awzy's own schema, not client data.
- One more generated secret in bootstrap, and the chart wires it into the Job and the reporter.

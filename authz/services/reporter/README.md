# awzy-reporter · fleet heartbeat

Sends this install's heartbeat to the corp portal every minute
([ADR 0026](../../../docs/adr/0026-install-registry-and-heartbeats.md)). Part of
[awzy-authz](../../).

The heartbeat is the only thing that leaves an install for awzy
([ADR 0009](../../../docs/adr/0009-no-vendor-access-to-client-data.md)), in the fixed schema of
`libs/heartbeat`: versions, readiness, and a few aggregates, nothing free-form. In a cluster it
reads awzy's workloads from the Kubernetes API through a read-only Role and scrapes each PDP's
`/metrics`. Every heartbeat is logged before it's sent, `--print` shows one without sending
it, and `AWZY_REPORTER_ENABLED=false` turns reporting off.

ClickHouse disk use and Kafka lag come from a ClickHouse user of its own, which can read only
`system.disks` sizes and `system.kafka_consumers`, and from Kafka's partition offsets: it never
reads decisions, events, or query text, and never consumes a topic
([ADR 0042](../../../docs/adr/0042-reporter-reads-clickhouse-and-kafka-offsets.md)). The
`awzy-clickhouse-migrate` Job creates that user on every sync.

Running it locally: [DEVELOPMENT.md](../../../DEVELOPMENT.md#fleet-reporting-adr-0026).

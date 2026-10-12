# awzy-clickhouse-migrate

ClickHouse schema for awzy-authz: numbered SQL migrations in `migrations/`, applied in order
and recorded in `schema_migrations`. Part of [awzy-authz](../../).

The decision log flows from the PDP to Kafka (`awzy.decisions.v1`) and into ClickHouse through
a Kafka-engine table and a materialized view
([ADR 0006](../../../docs/adr/0006-decision-log-through-kafka-into-clickhouse.md)). awzy-intake's
events take the same path (`awzy.events.v1` into `events`), in their own consumer group. Migrations may
use `{{placeholders}}` for settings that differ per install, such as the Kafka brokers as
ClickHouse itself reaches them.

Running it locally: [DEVELOPMENT.md](../../../DEVELOPMENT.md#local-stack).

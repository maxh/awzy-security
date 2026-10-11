# awzy-intake · events

Receives access logs, cloud audit logs (CloudTrail, Cloud Audit Logs, Azure Activity Log), proxy
logs, DLP and endpoint signals, and agent traces (OpenTelemetry). Part of
[awzy-authz](../../README.md).

Normalizes them into one event schema, enriches them with entity IDs, and writes them to Kafka.

- One normalizer per cloud audit-log format
- Kafka feeds ClickHouse through its Kafka table engine

## Built so far

| Endpoint | Takes |
| --- | --- |
| `POST /v1/events` | `{"events": [...]}` in awzy's own schema, for apps that log access themselves |
| `POST /v1/events/aws-cloudtrail` | A CloudTrail log file, `{"Records": [...]}` |

Each event becomes an `EventRecord` (`libs/model`) with an `event_` QID and UTC times, goes to
Kafka (`awzy.events.v1`, partitioned by principal), and lands in ClickHouse's `events` table next
to `decisions` (migration 5 in [awzy-clickhouse-migrate](../../jobs/clickhouse-migrate/README.md)).

- **Acknowledged, not fire-and-forget.** A request gets `202` only once Kafka has every event,
  and `503` if it can't, so a log shipper retries and nothing is lost. Retries can duplicate;
  CloudTrail's `eventID` is kept in `attributes` for removing them later.
- **All or nothing.** A batch (at most 1,000 events) is refused with `400` and the index of the
  first bad event, so a shipper never half-delivers.
- **Bearer token.** Shippers send `AWZY_INTAKE_TOKEN`, compared in constant time.
- **CloudTrail sessions of people roles are the person**
  ([ADR 0086](../../../docs/adr/0086-cloudtrail-sessions-of-people-roles-are-the-person.md)): an
  IAM Identity Center session, or a session of a role named in `AWZY_INTAKE_HUMAN_ROLES`, becomes
  the `Human` its session name names, with the ARN kept as `awsPrincipal`. Other callers stay as
  CloudTrail named them: `AwsPrincipal` with the ARN, or `AwsService` for a service acting on its
  own.

Not yet: mapping other roles to registered service accounts, Cloud Audit Logs and the Azure
Activity Log, DLP and endpoint signals, and OpenTelemetry agent traces.

Running it locally: [DEVELOPMENT.md](../../../DEVELOPMENT.md#events-awzy-intake).

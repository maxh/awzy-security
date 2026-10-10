# 0041. The corp portal keeps 30 days of heartbeat numbers in its Postgres

- Status: Accepted
- Date: 2026-10-09
- Amends: [0026](0026-install-registry-and-heartbeats.md)

## Context

0026 kept only each install's latest heartbeat and said health time series would move to
ClickHouse "when the fleet needs charts". Staff now need them: a latest snapshot can't show
whether PDP p99 has been creeping up all week, whether an install was degraded overnight, or
how long it stopped reporting. The corp portal runs in awzy's own account with Postgres and no
ClickHouse, and the fleet is a handful of installs.

## Decision

- Each heartbeat's numbers go into a `heartbeat_samples` table in the corp portal's Postgres, one
  row per heartbeat: decisions per second, PDP p99, error rate, Kafka lag, ClickHouse disk use,
  and how many reported services were fully ready. Nothing else from the heartbeat is kept
  there, and nothing the heartbeat schema doesn't already allow (0009, 0026).
- Rows are kept 30 days. Recording a heartbeat deletes that install's older rows.
- `Install.metricHistory(window: DAY | WEEK | MONTH)` returns the samples in 5-minute, 30-minute,
  or 2-hour buckets: the worst p99, error rate, lag, and disk use in each, average throughput,
  and whether any heartbeat in it was degraded. Buckets with no heartbeat are left out, so time
  an install wasn't reporting shows as a gap.
- The Install page charts them with a shared SVG line chart in `@awzy/ui`, rather than adding a
  charting library for five single-series charts.

## Consequences

- About 1,440 rows per install per day, 43,200 per install at the retention limit: fine for
  Postgres at tens of installs. Past a few hundred installs, or for retention measured in
  months, move this to ClickHouse as 0026 planned.
- Kafka lag and ClickHouse disk use chart as "not reported" until awzy-reporter measures them.
- Alerting on these numbers (an install degraded or not reporting) is still to be built.

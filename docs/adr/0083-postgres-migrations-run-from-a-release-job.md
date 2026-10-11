# 0083. Postgres migrations run from a release Job; services only check the schema

- Status: Accepted
- Date: 2026-10-11
- Amends: [0072](0072-deploys-are-zero-downtime.md) (zero-downtime deploys)

## Context

awzy-policy, awzy-context, and awzy-challenge migrated their own database as each pod started.
In an install that meant the first new pod migrated while old pods served, and a migration that
failed left that pod crash-looping, with the rollout stuck halfway and nothing saying why.
ClickHouse's schema already ran as a Job (`awzy-clickhouse-migrate`) before the services roll.

## Decision

- **Each service's own image applies its migrations,** run as `<binary> migrate`, so the
  migrations stay embedded with the code that needs them (`sqlx::migrate!`).
- **The chart runs it as a Job per database:** an Argo CD `Sync` hook in wave -1 with
  `BeforeHookCreation`, beside `awzy-postgres-databases`, retrying until its database exists. A
  failure stops the sync before any Deployment changes.
- **Services check, and don't migrate,** under `AWZY_POSTGRES_MIGRATE=check`, which the chart
  sets: they refuse to start unless every migration they embed is applied. More applied than they
  know is fine, since during a rollout the older version runs against the newer schema (0072's
  expand-and-contract rule still holds). Without the setting, as when run locally, a service still
  applies its own at start.
- **The corp portal** keeps migrating at start for now: it runs on ECS in awzy's account, not in
  installs, and a one-off migrate task there needs its own deploy step.

## Consequences

- A broken migration shows up as a failed Job in Argo CD, and the old version keeps serving.
- Running a service locally needs nothing new.
- `awzy_store::migrations` holds the check, for any client-side service that adds a database.

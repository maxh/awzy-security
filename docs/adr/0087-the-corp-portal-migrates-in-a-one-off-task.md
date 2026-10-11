# 0087. The corp portal migrates in a one-off task before each deploy

- Status: Accepted
- Date: 2026-10-11
- Amends: [0083](0083-postgres-migrations-run-from-a-release-job.md) (the corp portal's exception)

## Context

0083 moved the client-side services' Postgres migrations into a release Job and left the corp
portal migrating at start, because it runs on ECS in awzy's account and a migrate step there
needed its own deploy step. So a broken corp migration still crash-looped the new task until ECS's
circuit breaker rolled it back, and the registry proxy, which keeps its pull tokens in its own
schema of the same database, migrated at start too.

The check 0083 added lives in `awzy_store`, which portals/corp may not link (0009).

## Decision

- **`deploy-corp.sh` runs `awzy-corp-portal migrate` as a one-off ECS task** with the new task
  definition, on the service's network, before either service changes. It applies the corp
  portal's migrations and the registry schema's. If it fails, the deploy stops and the old
  revisions keep serving.
- **Both ECS services only check**, with `AWZY_POSTGRES_MIGRATE=check` in their task definitions,
  as the client-side services do in an install. Run locally, they still migrate at start.
- **The check moves to its own crate, `awzy-migrations`** (`libs/migrations`). It holds no data and
  links nothing client-side, so both sides use it, and `awzy_store` no longer carries it.

## Consequences

- No awzy service migrates at start when deployed.
- The deploy role can run tasks from the corp portal's task definition family in the corp
  cluster, and describe them.
- A deploy takes about a minute longer: Fargate starts the migrate task before the services roll.

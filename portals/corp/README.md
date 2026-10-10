# Corp portal · awzy-corp-portal

awzy staff's view of the fleet, at https://corp.awzysecurity.com. It runs in awzy's own AWS
account ([ADR 0035](../../docs/adr/0035-vendor-services-on-ecs-deployed-by-ci.md)) and never
holds client data ([ADR 0009](../../docs/adr/0009-no-vendor-access-to-client-data.md)). Part of
the [portals](../README.md).

| Page | Shows |
| --- | --- |
| Installs | Every install, its version, and whether it's healthy |
| Install | One install's services and 30 days of its heartbeat numbers |
| Releases | Which chart versions are running, and where: the rollout |

- **`backend/`** (`awzy-corp-portal`): GraphQL at `/graphql`, and `/v1/heartbeat`, where each
  install's awzy-reporter sends its fixed-schema heartbeat with its own token
  ([ADR 0026](../../docs/adr/0026-install-registry-and-heartbeats.md)). Install records and
  heartbeat history are in Postgres ([ADR 0041](../../docs/adr/0041-fleet-health-history-in-the-corp-portal-postgres.md)).
  `awzy-corp-user` makes a staff account.
- **`frontend/`** (`@awzy/corp-portal`): the React app.
- **[`registry/`](registry/README.md)** (`awzy-registry`): the pull-only proxy installs pull
  images and the chart from.

Staff sign in with a username and password, a stopgap for demos and testing until awzy has its
own IdP ([ADR 0025](../../docs/adr/0025-corp-portal-passwords-for-demos-and-testing.md)).

CI deploys the backend and the registry on every merge to `main`. Running it locally:
[DEVELOPMENT.md](../../DEVELOPMENT.md#corp-portal-demos-and-testing-only).

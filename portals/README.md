# awzy portals

The two web apps people use to see and run awzy. They have the same shape, a Rust backend
serving GraphQL at `/graphql` and a Vite + React + Relay + Mantine frontend
([ADR 0012](../docs/adr/0012-portal-stack.md)), but they run in different places and must never
share data.

| Folder | Who uses it | Runs in |
| --- | --- | --- |
| [client/](client/README.md) | The client's security team: decisions, policies, enforcement modes, the directory | Each client's cluster, next to awzy-authz |
| [corp/](corp/README.md) | awzy staff: the fleet of installs, their health, and releases; plus the release registry | awzy's own AWS account |
| [common/](common/README.md) | TypeScript packages both frontends use | — |

The corp portal may never see client data
([ADR 0009](../docs/adr/0009-no-vendor-access-to-client-data.md)): what it knows about an
install comes only from that install's fixed-schema heartbeat. Bazel visibility
(`//tools:client_side`) fails the build if anything under `corp/` depends on `authz/` or
`libs/store`.

Running them locally, and working on a frontend against a deployed backend:
[DEVELOPMENT.md](../DEVELOPMENT.md#frontends).

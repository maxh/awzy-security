# awzy-authz

The services that run in each client's cluster and make and enforce authorization decisions,
the jobs that prepare their data stores, the libraries they share, and the demo's synthetic
company. They hold the client's data, so nothing here runs in awzy's own
accounts, and only the fixed-schema heartbeat leaves an install
([ADR 0002](../docs/adr/0002-single-tenant-installs-in-client-clouds.md),
[ADR 0009](../docs/adr/0009-no-vendor-access-to-client-data.md)).

The folders say what each crate is ([ADR 0040](../docs/adr/0040-authz-grouped-into-services-jobs-and-libs.md),
[ADR 0063](../docs/adr/0063-demo-only-code-in-authz-demo.md)):
`services/` run continuously, `jobs/` run to completion once per release, `libs/` are crates
the others build on, and `demo/` is for demo installs and tests only, which nothing else may
depend on.

All services are Rust binaries in the Bazel monorepo, each built into its own container image
and installed together by one Helm umbrella chart (`deploy/helm/awzy`). None of them imports a
cloud SDK directly; they go through the `libs/cloud` traits.

## Services

| Service | Role |
| --- | --- |
| [awzy-pdp](services/pdp/) | Hot path: decides allow, deny, or challenge for one action on one resource |
| [awzy-gateway](services/gateway/) | Enforcement point: MCP proxy today, HTTP reverse proxy next |
| [awzy-policy](services/policy/) | Versioned, signed policy bundles and each scope's enforcement mode |
| [awzy-context](services/context/) | World model: people, groups, service accounts, resources, and the attribute feed |
| [awzy-connectors](services/connectors/) | Sync from systems of record (IdP, HR, work tracking) into awzy-context |
| [awzy-intake](services/intake/) | Normalizes logs, audit trails, and agent traces into events on Kafka |
| [awzy-reasoner](services/reasoner/) | Slow path: activity baselines, investigations, risk attributes |
| [awzy-challenge](services/challenge/) | Challenges (justification, security key, approval) and containments |
| [awzy-reporter](services/reporter/) | The install's heartbeat to the corp portal |

## Jobs

| Job | Does |
| --- | --- |
| [awzy-clickhouse-migrate](jobs/clickhouse-migrate/) | Applies the ClickHouse schema before the services roll out |

## Demo

Never on a real install. Bazel refuses a dependency on these from anywhere else.

| Crate | Does |
| --- | --- |
| [awzy-synthetic](demo/synthetic/) | Larkspur Capital, the demo's synthetic company: provisions it, serves its stand-in MCP servers (larkspur-drive, larkspur-admin), and drives its activity and scenarios |

## Libraries

| Crate | For |
| --- | --- |
| [classify](libs/classify/) | Placing resources in a sensitivity tier from labels, content markings, names, and folders |
| [identity](libs/identity/) | Who is calling, from a validated bearer token |
| [proto](libs/proto/) | Protobuf definitions and generated gRPC types |
| [service-auth](libs/service-auth/) | Services in an install proving who is calling, with Kubernetes service-account tokens |
| [signing](libs/signing/) | Signing and verifying policy bundles |

How these fit together for one decision: [Decision path](../docs/plans/platform-plan.md#decision-path).
Which component of the Beyond Zero paper each service implements:
[Beyond Zero mapping](../docs/plans/platform-plan.md#beyond-zero-mapping). What's built so far:
[Built so far](../docs/plans/platform-plan.md#built-so-far). Building and running them:
[DEVELOPMENT.md](../DEVELOPMENT.md).

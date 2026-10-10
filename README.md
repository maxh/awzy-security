<p align="center"><img src="docs/brand/kangaroo.svg" alt="awzy's kangaroo" width="120"></p>

# awzy

Beyond Zero authorization for the AI era: per-action, per-resource decisions for humans and
agents, deployed single-tenant inside each client's cloud (AWS, Google Cloud, or Azure).

Read more:

- [Platform plan](docs/platform-plan.md): the architecture, and what's built so far
- [Deployment plan](docs/deployment-plan.md): how installs reach a client's cloud
- [Demo plan](docs/demo-plan.md): the synthetic company the demo and tests run on
- [Decision records](docs/adr/): one file per architecture decision
- [Beyond Zero](docs/beyond-zero.pdf), the paper awzy implements (Valente and Zalewski, ACM 2026, CC BY 4.0)

## Layout

| Path | What | Runs in |
| --- | --- | --- |
| [`authz/`](authz/README.md) | [`services/`](authz/services) ([pdp](authz/services/pdp/README.md), [gateway](authz/services/gateway/README.md), [policy](authz/services/policy/README.md), [context](authz/services/context/README.md), [connectors](authz/services/connectors/README.md), [intake](authz/services/intake/README.md), [reasoner](authz/services/reasoner/README.md), [challenge](authz/services/challenge/README.md), [reporter](authz/services/reporter/README.md)), [`jobs/`](authz/jobs) ([clickhouse-migrate](authz/jobs/clickhouse-migrate/README.md), [synthetic](authz/jobs/synthetic/README.md): the demo's synthetic company), [`libs/`](authz/libs) ([classify](authz/libs/classify/README.md), [identity](authz/libs/identity/README.md), [proto](authz/libs/proto/README.md), [service-auth](authz/libs/service-auth/README.md), [signing](authz/libs/signing/README.md)) | client cluster |
| [`portals/client/`](portals/client/README.md) | [`backend`](portals/client/backend) (GraphQL at `/graphql`, MCP at `/mcp`, serves the frontend), [`frontend`](portals/client/frontend) (Vite + React + Relay + Mantine) | client cluster |
| [`portals/corp/`](portals/corp/README.md) | same shape, for awzy staff; never holds client data; plus [`registry`](portals/corp/registry/README.md), the release registry proxy | vendor account |
| [`portals/common/`](portals/common/README.md) | TypeScript packages both portal frontends use: [`@awzy/ui`](portals/common/ui), [`@awzy/relay`](portals/common/relay) |  |
| [`libs/`](libs) | shared Rust crates: [model](libs/model), [server](libs/server), [telemetry](libs/telemetry), [cloud](libs/cloud), [llm](libs/llm), [store](libs/store), [portal](libs/portal), [heartbeat](libs/heartbeat) |  |
| [`policies/`](policies) | base Cedar [schema](policies/awzy.cedarschema) and [policies](policies/base.cedar) |  |
| [`docs/`](docs) | the plans above and the [decision records](docs/adr) |  |
| [`tools/`](tools) | Bazel macros ([`awzy_service`](tools/rust.bzl), [`awzy_rust_library`](tools/rust.bzl)), the [`client_side`](tools/BUILD.bazel) visibility group, and CI scripts ([`tools/ci/`](tools/ci)) |  |
| [`deploy/`](deploy) | the [Helm chart](deploy/helm/awzy), [Terraform](deploy/terraform) for installs and the vendor side, and the [local stack](deploy/local/compose.yaml) |  |

`portals/corp` cannot depend on `authz/` or `libs/store`. Bazel visibility (`//tools:client_side`)
fails the build if it tries.

## Development

[DEVELOPMENT.md](DEVELOPMENT.md) covers setup (including the pre-commit hook), building and
testing, the local stack, local HTTPS, and running each service and portal. Coding agents also
read [AGENTS.md](AGENTS.md).

## Installing

[INSTALL.md](INSTALL.md) is the runbook for installing awzy in your own cloud account: the cloud
layer, bootstrap, DNS and your IdP, and checking the install.

## Not done yet

- linux/arm64 images: the Zig linker rejects rustc's `--fix-cortex-a53-843419` flag (see `tools/rust.bzl`).
- Optimized (`-c opt`) release builds.
- Real implementations behind the remaining service skeletons; the corp portal has no MCP tools yet.

What each service does today, in more detail: [Built so far](docs/platform-plan.md#built-so-far).

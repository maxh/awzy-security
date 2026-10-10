<p align="center"><img src="docs/brand/kangaroo.svg" alt="awzy's kangaroo" width="120"></p>

# awzy

Beyond Zero authorization for the AI era: per-action, per-resource decisions for humans and
agents, deployed single-tenant inside each client's cloud (AWS, Google Cloud, or Azure).

Architecture plan: [docs/platform-plan.md](docs/platform-plan.md). Deployment plan: [docs/deployment-plan.md](docs/deployment-plan.md).
Demo and synthetic data: [docs/demo-plan.md](docs/demo-plan.md). The Beyond Zero paper: [docs/beyond-zero.pdf](docs/beyond-zero.pdf) (Valente and Zalewski, ACM 2026, CC BY 4.0).
Decision records: [docs/adr/](docs/adr/README.md).

## Layout

| Path | What | Runs in |
| --- | --- | --- |
| [`authz/`](authz/README.md) | `services/` (pdp, gateway, policy, context, connectors, intake, reasoner, challenge, reporter), `jobs/` (clickhouse-migrate, synthetic: the demo's synthetic company), `libs/` (identity, proto, signing) | client cluster |
| [`portals/client/`](portals/client/README.md) | `backend` (GraphQL at `/graphql`, MCP at `/mcp`, serves the frontend), `frontend` (Vite + React + Relay + Mantine) | client cluster |
| [`portals/corp/`](portals/corp/README.md) | same shape, for awzy staff; never holds client data; plus `registry`, the release registry proxy | vendor corp account |
| `libs/` | shared Rust crates: model, server, telemetry, cloud, llm, store, portal | |
| [`portals/common/`](portals/common/README.md) | TypeScript packages both portal frontends use: `@awzy/ui`, `@awzy/relay` | |
| `policies/` | base Cedar schema and policies | |
| `docs/` | platform plan, deployment plan, and architecture decision records (`docs/adr/`) | |
| `tools/` | Bazel macros (`awzy_service`, `awzy_rust_library`) and the `client_side` visibility group | |

`portals/corp` cannot depend on `authz/` or `libs/store`. Bazel visibility (`//tools:client_side`)
fails the build if it tries.

## Development

[DEVELOPMENT.md](DEVELOPMENT.md) covers setup (including the pre-commit hook), building and
testing, the local stack, local HTTPS, and running each service and portal. Coding agents also
read [AGENTS.md](AGENTS.md).

## Not done yet

- linux/arm64 images: the Zig linker rejects rustc's `--fix-cortex-a53-843419` flag (see `tools/rust.bzl`).
- Optimized (`-c opt`) release builds.
- Real implementations behind the remaining service skeletons; the corp portal has no MCP tools yet.

What each service does today, in more detail: [Built so far](docs/platform-plan.md#built-so-far).

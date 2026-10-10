# 0010. Bazel monorepo with a capped setup budget

- Status: Accepted
- Date: 2026-10-08

## Context

The system spans Rust services, two TypeScript frontends, gRPC and GraphQL schemas, container
images, a Helm chart, and Terraform. The corp side must never depend on client-side code, and a
release must deploy services built from one tested commit.

## Decision

- One repository built by Bazel 8 (bzlmod): rules_rust with crate_universe reading the Cargo
  workspace and the single `Cargo.lock`, aspect_rules_js with a pnpm workspace, rules_oci on
  distroless bases. No Dockerfiles.
- Every service uses the `awzy_service` macro in `tools/rust.bzl`, which declares its library,
  unit tests, binary, and `:image_index`.
- Prefer pure-Rust crates so builds need no C libraries: `protox` + tonic instead of `protoc`,
  `rskafka` instead of `rdkafka`.
- Linux binaries are cross-compiled with the hermetic Zig toolchain, so linux/amd64 images build on
  macOS without Docker. arm64 is deferred: the Zig linker rejects `--fix-cortex-a53-843419`.
- Bazel gets a fixed setup budget. If it runs over, Phase 1 ships on Cargo and pnpm workspaces with
  the same layout and a dependency lint for the boundary.

## Consequences

- The Cargo workspace stays valid, so `cargo check` and rust-analyzer work.
- A breaking GraphQL schema change fails the frontend build in the same commit.
- `tests/` integration-test directories are not built by `awzy_service` yet.

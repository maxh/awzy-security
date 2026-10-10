# AGENTS.md

Guidance for coding agents (Claude Code, Codex, and others) working in this repository.
`CLAUDE.md` is a symlink to this file. People should start with [README.md](README.md); local
setup is in [DEVELOPMENT.md](DEVELOPMENT.md).
Skills for common tasks (linting, the local stack, a new service, an ADR) are in
`.agents/skills/`; `.claude/skills` links to them.

## What this is

awzy implements Google's "Beyond Zero" authorization: per-action, per-resource decisions for
humans and AI agents, installed single-tenant in each client's cloud. The plans are
[docs/platform-plan.md](docs/platform-plan.md) and [docs/deployment-plan.md](docs/deployment-plan.md).
Decisions are recorded in [docs/adr/](docs/adr/README.md). Read the relevant ADR before
changing something it covers, and don't re-propose a decision an ADR settled (for example,
keeping Kafka) without new information.

## Layout and boundaries

Code lives with the product that owns it (ADR 0011), never in repo-wide top-level folders:
`authz/` (client side, split into `services/`, `jobs/` that run once per release, and `libs/`
for client-side code only; ADR 0040), `portals/client/`, `portals/corp/`, `portals/common/`
(shared TypeScript), `libs/` (shared Rust), `policies/` (Cedar), `tools/` (Bazel macros, CI
scripts, dev IdP). Shared code goes in a folder called `common`, not `shared`.

`portals/corp` runs in awzy's own account and must never touch client data (ADR 0009). Bazel
visibility (`//tools:client_side`) fails the build if it depends on `authz/` or `libs/store`.
Don't widen that visibility to make something compile.

## Build and test

Everything builds with Bazel through bazelisk (version in `.bazelversion`). Bazel downloads the
Rust, Node, and pnpm toolchains, so `cargo`, `rustfmt`, and `pnpm` may not be installed.

```sh
bazel test //...                                   # Rust tests, Relay validation, TS typechecks
docker compose -f deploy/local/compose.yaml up -d --wait
bazel test --config=local-stack //...              # plus the Postgres/Kafka/ClickHouse tests
bazel build --config=lint //...                    # clippy
```

- Integration tests skip (and pass) when their `AWZY_TEST_*` variables are unset. A green run
  without `--config=local-stack` didn't exercise the database code.
- Rust dependencies are declared once in the root `Cargo.toml` `[workspace.dependencies]` and
  used with `x.workspace = true`. After changing a dependency or adding a crate, run
  `CARGO_BAZEL_REPIN=1 bazel test //...` to update `Cargo.lock` and `MODULE.bazel.lock`, and
  commit both.
- First-party Rust deps go in the `deps` of the package's `BUILD.bazel` (`awzy_service` /
  `awzy_rust_library` in `tools/rust.bzl`) as well as its `Cargo.toml`.
- pnpm runs with `hoist=false`: every TypeScript package declares every dependency it imports.
- GraphQL schemas are generated from the Rust backends. After changing a resolver, run
  `bazel run //portals/<client|corp>/frontend:update_schema`, then `pnpm relay` in that frontend
  (after `pnpm install`), or `bazel test` fails on the stale `schema.graphql` and `__generated__`
  files.

## Conventions

- **IDs** are typed per entity and written as QIDs, `<entity_type>_<uuid>` with UUIDv7
  (`decision_…`, `policy_bundle_…`; ADR 0013). Use the QID wherever an ID leaves a service;
  storage keeps native UUID columns. New entity: add it with the `qid!` macro in
  `libs/model/src/qid.rs`.
- **Time** is always UTC: `DateTime<Utc>` in Rust, the `UtcTimestamp` scalar in GraphQL (never
  `DateTime`), and `google.protobuf.Timestamp` in protobuf. The exception is a field inside a
  signed payload, which uses integer Unix time so the signed bytes are unambiguous
  (`requested_at_unix` on a break-glass request).
- **IDs on the wire** (protobuf, GraphQL, JSON): a field holding one of awzy's own QIDs is named
  `_qid` (`string decision_qid`, `challengeQid`; a message's own is `qid`), so it can't be
  mistaken for an ID from the client's systems, which keeps `_id` (`person_id`, `EntityRef.id`).
  GraphQL objects keep `id`, which Relay requires (ADR 0058).
- **Names** are specific and easy to grep (`ClientPortalSchema`, `CorpPortalQuery`, not `Query`).
  The products are "portals", never "consoles".
- **Rust errors:** `thiserror` types in libraries, `anyhow` in binaries and at the edges.
- **Tests** sit next to the code in `#[cfg(test)] mod tests` and read as statements of behavior
  (`delegation_never_exceeds_the_person`).
- **Comments and docs** explain why, briefly, in plain sentences. Match the comment density of the
  surrounding code. Doc comments on public items say what they're for, not how.
- **Architecture decisions** get an ADR: copy the shape of a recent one in `docs/adr/` and take
  the next number (`ls docs/adr | tail -1`). Accepted ADRs aren't edited; a change of mind is a
  new ADR that supersedes or amends the old one.
- **Configuration names** start with `AWZY_` (`AWZY_<COMPONENT>_<SETTING>`); the vendor side's,
  including CI variables, with `AWZY_CORP_`; test-only inputs with `AWZY_TEST_`. Lists of keys
  are sorted alphabetically (ADR 0053).
- **Commits:** `<area>: <what changed>` as the subject, then a body of short bullets on what and
  why. Reference ADRs by number.

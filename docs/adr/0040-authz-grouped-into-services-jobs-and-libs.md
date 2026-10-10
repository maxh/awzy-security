# 0040. authz/ is grouped into services, jobs, and libs

- Status: Accepted
- Date: 2026-10-09
- Amends: [0011](0011-ownership-based-layout.md)

## Context

0011 put each awzy-authz crate directly under `authz/`. By now that folder mixes three kinds of
thing with nothing in their names to tell them apart: nine long-running services
(`awzy-pdp`, `awzy-gateway`, …), one release-time Job that applies the ClickHouse schema and
exits (`awzy-clickhouse-migrate`, which looks like a service because the same Bazel macro builds
its image), and three libraries (`identity`, `proto`, `signing`). Reading the folder, or the
Helm chart next to it, means already knowing which is which.

## Decision

- `authz/services/<name>`: long-running services, one image and one Deployment each.
- `authz/jobs/<name>`: images that run to completion once per release. The ClickHouse migrations
  move to `authz/jobs/clickhouse-migrate`, and the demo's synthetic company
  ([0039](0039-synthetic-organization-for-demo-and-tests.md)) to `authz/jobs/synthetic`.
- `authz/libs/<name>`: libraries that only client-side code uses. They stay under `authz/`, not
  the top-level `libs/`, so `//tools:client_side` keeps the corp portal from depending on them.
- 0011 otherwise stands: code lives with the area that owns it, and `authz/` is still that area.
  This splits the area by what each crate is, not the repository by kind of file.
- Crate, binary, and image names don't change. Labels do (`//authz/pdp` becomes
  `//authz/services/pdp`).

Rejected: keeping the flat layout and explaining it in `authz/README.md`. The tables there were
a workaround for a layout that didn't say what it was. Rejected: `authz/common/` for the
libraries. "common" names shared TypeScript in `portals/common/`, while the top-level `libs/`
already means shared Rust crates.

## Consequences

- Every Bazel label, Cargo workspace path, and doc path under `authz/` changed in one commit.
  Accepted ADRs keep the old paths as written.
- Open branches that touch `authz/` need a rebase; git follows the renames.
- A new crate's folder has to answer "service, job, or library?" first.

# 0011. Code lives with the area that owns it

- Status: Accepted; amended by [0040](0040-authz-grouped-into-services-jobs-and-libs.md)
- Date: 2026-10-08

## Context

The scaffold started with repo-wide folders (`proto/`, `web/`) and layer-first portal folders. That
puts client-side and corp-side code next to each other and makes ownership unclear.

## Decision

- Code lives with the product or area that owns it. There are no repo-wide top-level folders for
  a kind of file. `proto/` moved to `authz/proto/`, `web/` moved to `portals/common/`, and the
  ClickHouse schema lives in `authz/clickhouse/`.
- Portals are grouped by audience: `portals/client/` and `portals/corp/`, each with `backend/`
  and `frontend/`. Shared frontend packages are in `portals/common/` ("common", not "shared").
  Shared Rust crates are in `libs/`.
- "Portal", not "console", everywhere.
- Bazel visibility (`//tools:client_side`) fails the build if anything under `portals/corp/` depends
  on `authz/` or `libs/store`.

## Consequences

- Each portal folder matches one deployment and one data boundary, enforced by the build.

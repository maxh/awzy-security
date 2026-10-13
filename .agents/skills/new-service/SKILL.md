---
name: new-service
description: Add a new deployable Rust service (awzy-<name>) to the monorepo, wired through Cargo, Bazel, the heartbeat, the Helm chart, and the release registry. Use when asked to create a new service, binary, or crate that ships as an image.
---

# Add a service

A service is a library crate plus a binary and an image, made by the `awzy_service` macro in
`tools/rust.bzl`. Use an existing small service (`authz/services/reporter`, `authz/services/challenge`) as the
template, and put it with the area that owns it (`authz/services/`, `portals/<client|corp>/`;
ADR 0011, 0040). Something that runs once per release and exits goes in `authz/jobs/`, and anything only for
demo installs and tests in `authz/demo/` (ADR 0063).
First ask whether the job could live in an existing service instead. Every service is one more
thing a client's team operates in an account awzy can't sign in to: pods to keep ready, an alert,
and a section in the docs site's troubleshooting guide saying what fails without it.
A significant new service usually deserves an ADR first (see the write-adr skill).

1. **Crate.** `authz/services/<name>/Cargo.toml` with `name = "awzy-<name>"`, workspace `version`,
   `edition`, `publish`, `license`, and only `x.workspace = true` dependencies. Add any new
   third-party crate to `[workspace.dependencies]` in the root `Cargo.toml` first.
2. **Workspace.** Add the path to `members` in the root `Cargo.toml`, and
   `awzy-<name> = { path = "..." }` to its first-party list if other crates will depend on it.
3. **Code.** `src/lib.rs` (crate docs: what the service does and which ADR it implements; a
   `router()`), and `src/main.rs`, which is only wiring:
   `awzy_telemetry::init("awzy-<name>")`, configuration from `AWZY_*` variables, then
   `awzy_server::serve("awzy-<name>", router).await`. That gives `/healthz`, `/readyz`, and
   `/version` for free.
4. **Bazel.** `BUILD.bazel` with `awzy_service(name, crate_name = "awzy_<name>",
   binary_name = "awzy-<name>", deps = [first-party targets])`. Client-side services set
   `visibility = ["//tools:client_side"]`; corp code may never depend on them (ADR 0009).
5. **Locks.** `CARGO_BAZEL_REPIN=1 bazel query //... && bazel query //...` updates `Cargo.lock` and
   `cargo-bazel-lock.json` for the new crate and any new dependency; commit both. On a pull
   request, `fix-prs` does it for you.
6. **Heartbeat.** If it runs in client installs, add it to the closed `Service` enum in
   `libs/heartbeat/src/lib.rs` (variant, serde rename, `ALL`, `as_str`), or the reporter can't
   report it.
7. **Release.** Add `"awzy-<name>"` to `local.images` in `deploy/terraform/corp/build/main.tf`,
   or CI's publish step fails for want of an ECR repository. The repository exists once someone
   applies that Terraform; say so in the PR.
8. **Install.** A Helm template in `deploy/helm/awzy/templates/` modeled on a similar service,
   values in `values.yaml` and `values-example.yaml`, then
   `helm lint --strict deploy/helm/awzy -f deploy/helm/awzy/values-example.yaml`.
9. **Docs.** The layout table in `README.md`, and how to run it locally in `DEVELOPMENT.md` if
   it has its own setup. For clients: its row in the docs site's `reference/services.md`, and its
   section in `guides/troubleshooting.md` (what fails without it, its alerts, what to do).
10. **Check.** `bazel test //authz/services/<name>/...`, `bazel build //authz/services/<name>:image_index`, and
    the lint-fix skill.

# 0036. Bazel's remote cache is BuildBuddy

- Status: Accepted
- Date: 2026-10-09

## Context

A green CI run took about 9.5 minutes, and every build action in it was already a cache hit.
About 4 of those minutes went to downloading and re-uploading Bazel's disk cache as one archive
through GitHub's cache. Several sessions push often, so CI time adds up, and laptops rebuild
Rust from scratch after a clean or a fresh checkout.

## Decision

- Bazel uses BuildBuddy's hosted remote cache (free tier), org `awzy`: `--config=remote` in
  `.bazelrc`, which fetches only top-level outputs (`--remote_download_toplevel`).
- CI writes the key from the `BUILDBUDDY_API_KEY` secret into `.bazelrc.user` and turns the
  remote cache on. Without the secret (forks), it falls back to the disk cache in GitHub's cache.
- Laptops opt in with their own key in the gitignored `.bazelrc.user`.
- Organization settings: builds can't be made public, members aren't added by email domain, and
  the "Fix with AI" button is off.

## Consequences

- A third party stores build outputs (compiled binaries, images, test logs) and build event
  streams for a private codebase. Acceptable for now: no client data passes through CI. Revisit
  before CI handles anything client-specific, or move to `bazel-remote` in `awzy-build`
  (the platform plan's option) when self-hosting is worth it.
- Keys are org-wide; rotate them in BuildBuddy and in the GitHub secret if one leaks.
- Each CI run and local build links to its BuildBuddy page, which makes slow or failing builds
  easier to look into.

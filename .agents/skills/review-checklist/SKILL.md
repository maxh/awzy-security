---
name: review-checklist
description: Review your own change against awzy's conventions before committing or opening a PR. Use before every commit of code, and when asked to review a change.
---

# Before you commit

Run through these; each one has caught a real mistake.

1. **Formatting and lint:** `tools/fmt`, then `pnpm lint` for TypeScript. Bazel's clippy runs in
   CI with no bare `unwrap()`, `panic!`, or `unsafe` outside tests.
2. **Only your files changed:** `git status`. A formatter run over a directory reformats files you
   didn't mean to touch; put those back.
3. **Generated files:** schemas regenerated, not hand-edited (see `add-graphql-field`); Relay
   output never committed; `schema.sql` regenerated with `tools/db-schema` after a migration.
4. **Typed boundaries:** no `value["field"]` reads of another service's response, no new hyper or
   Kubernetes client (see `call-internal-service`), no `as` in TypeScript.
5. **IDs and times:** awzy's IDs are typed QIDs (`Qid<T>` in GraphQL, `[(awzy.authz.v1.qid)]` in
   protobuf); times are UTC.
6. **Errors:** typed in libraries, not `Result<_, String>`; nothing logs a credential.
7. **Tests:** a behavior change has a test named for the behavior; integration tests that need the
   local stack skip without it, so say whether you ran them.
8. **Docs:** public docs never cite ADRs; a decision gets an ADR; nothing deployment-specific
   (see `keep-deployments-out`).
9. **`tools/ci/check-quality-ratchets.sh`** passes: no count is higher than on main.

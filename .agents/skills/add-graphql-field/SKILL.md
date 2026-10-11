---
name: add-graphql-field
description: Add or change a field, argument, input, or type in either portal's GraphQL API. Use whenever a portal backend's schema changes.
---

# Adding to a portal's GraphQL API

Both portals' schemas are generated from the Rust backends and held to conventions by
`tools/ci/check-graphql-schemas.mjs` (ADR 0100). Get these right in the Rust; the schema follows.

## Types

- **awzy's own IDs** are `awzy_portal::Qid<T>` for the entity's typed ID (`Qid<ChallengeId>`), and
  the field is named `qid` or `<entity>Qid`. The resolver then gets the entity it expects, and any
  other is refused before it runs. `ID` is only an object's own `id` and `node(id)`.
- **An object with an awzy ID** has `id: ID!` and implements `Node`: add it to the `Node` enum in
  the portal's `node.rs` and route its `QidEntity` in `node(id)` (the match is exhaustive, so the
  compiler asks). Its `id` resolver is `pub`.
- **IDs from the client's systems** (people, groups, service accounts, resources) are `String`,
  named `<thing>Id`, never `Qid`.
- **Times** are `UtcTimestamp`, and fields ending in `At` are times.
- **JSON** is `JsonObject` (`JSONObject`) for an object the frontend reads, or `JsonString`
  (`JSONString`) for text that must stay as it is. Never a `String` holding JSON.
- **Who did something** is an `Actor`, never a string.

## Access

- Every resolver starts with `require(ctx, action, area)`.
- An error says what kind it is: `ErrorCode::InvalidInput.error("...")` for bad input,
  `ErrorCode::NotFound` for a missing ID, `ErrorCode::Unavailable` when a service didn't answer.
  `require` already codes denials and challenges. A bare string error has no code.
- A field more sensitive than its area goes in `fields.rs`'s list and is nullable, so a policy can
  hide it (ADR 0103); the frontend shows null as `<Hidden />`.
- A change policy might want a security key for gets its own Cedar action, asked with
  `action_challenge`, and returns a `challengeQid` the page passes before retrying.

## The schema file

Don't hand-edit `schema.graphql`: run `bazel run //portals/<client|corp>/frontend:update_schema`,
or push and let the fix-prs workflow regenerate it. Then `pnpm relay` in the frontend.

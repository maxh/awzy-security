# 0062. A person's ID on the wire is `human_id`

- Status: Accepted
- Date: 2026-10-10
- Amends: [0058](0058-qid-fields-are-named-qid-on-the-wire.md) (renames its `person_id` example),
  and makes a second one-time exception to
  [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (no edits to `v1`)

## Context

0058 kept `_id` for IDs from the client's systems and named `person_id` as one. Every
`person_id` holds the ID of a `Human` entity (the IdP's user name, `Human::"alice"`), but
nothing in the name says so: a reader sees `Human` in policies and `person_id` on the wire, and
has to guess they're the same thing, or that `person_id` should have been a QID.

A person isn't one of awzy's records, so it isn't a QID. awzy-context's SCIM `id` is a UUID awzy
assigns, but only because SCIM requires one: policies, tokens, and every other service name a
person by the IdP's user name, and an install without SCIM has no such record at all.

All the `person_id` fields are on awzy-challenge's security key messages (0059), which nothing
outside this repository is generated from yet.

## Decision

- A field holding a `Human` entity's ID is named for the entity type: `human_id` in protobuf and
  JSON, `humanId` in GraphQL. It is not a QID.
- Fields named for a role keep their names (`answerer_id`, `on_behalf_of`): the role says more
  than the type.
- **Second one-time exception to 0045:** the `v1` fields are renamed in place, as 0058's were.
  Field numbers don't change, so binary gRPC between old and new builds still works.
- Not renamed: awzy-challenge's `person_id` columns in Postgres, as 0058 left storage alone.

## Consequences

- `human_id` reads as "the ID of a `Human`", so the wire, the Cedar schema, and the docs use one
  name for one entity.
- CI's `buf breaking` check fails on the pull request that makes this change, and passes once
  it's on `main`.

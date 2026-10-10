# 0060. Who did something is a typed actor, not a string

- Status: Accepted
- Date: 2026-10-10
- Amends: [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (adds fields to
  `v1`, and deprecates others for `v2`)

## Context

Every record of who did something (a draft's `created_by`, a publish's `published_by`, a
containment's `created_by` and `lifted_by`, a challenge's `decided_by`, an enforcement switch's
`changed_by`, a service account's `created_by`) was a free-form string. What went in it varied:
a person's IdP ID (`alice`), a service account acting for one (`sales-genie for alice`),
`Awzy Bot for alice`, `break-glass:sha256:…`, `awzy-synthetic`, and `unknown`. Nothing said
which, so a reader couldn't tell a person from a service, couldn't look either up, and couldn't
rely on the format. Once awzy-reasoner contains accessors on its own, the difference between a
person's decision and awzy's matters to whoever reviews it.

0058 settled that these aren't QIDs: people and services aren't awzy's records. They're
entities, and awzy already writes entities as `EntityRef` (`type`, `id`).

## Decision

- **An actor is an entity, and the person it acted for, if any**: `awzy_model::Actor`, and
  `awzy.authz.v1.Actor { subject, on_behalf_of }` on the wire. The same shape as an accessor.
- **Its type says what it is**: `Human` (the client IdP's ID), `ServiceAccount`, `AwzyService`
  (one of awzy's own: `awzy-reasoner`, `awzy-bot`, `awzy-publish`), `BreakGlassKey` (a key's
  fingerprint; who held it isn't known), or `Staff` (a corp portal user name). Awzy Bot drafting
  for alice is `AwzyService::awzy-bot` on behalf of `Human::alice`.
- **Protobuf `v1` gains a typed field next to each string**, `…_by_actor`, and the strings are
  deprecated, removed in `v2`. Services read the typed field and fall back to the string from
  callers that predate it; they fill both in responses, the string as the actor's text form
  (`Human::alice`, `ServiceAccount::sales-genie for Human::alice`). That is additive, so 0045
  holds without an exception.
- **GraphQL returns an `Actor` object** (`type`, `id`, `onBehalfOf`, `label`) wherever a field
  said who did something. The portals' schemas aren't a public contract, so these change type in
  place.
- **Storage keeps its text columns**, holding the text form. Rows written before this are read
  by the same parser, which knows every old format, so nothing is migrated. Fields that only ever
  name a person answering for themselves (a challenge's `answered_by`, `decided_by`) keep the
  person's bare ID, which reads as `Human`, because approvals and answers are checked against
  that ID.
- **`x-awzy-actor`** (the client portal to awzy-context) carries the text form.

## Consequences

- The portals show `alice` or `sales-genie for alice` from `label`, and can link a `Human` to the
  directory and a `ServiceAccount` to its registration.
- An actor type outside the five above is read as a person's ID, the old behavior, rather than
  failing. Adding a type means adding it to the parser.
- `v2` of the protobuf API drops the string fields and renames `…_by_actor` to `…_by`.

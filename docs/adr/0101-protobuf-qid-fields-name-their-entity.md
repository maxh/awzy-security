# 0101. Protobuf QID fields name their entity

- Status: Accepted
- Date: 2026-10-11
- Amends: [0058](0058-qid-fields-are-named-qid-on-the-wire.md) (QIDs in protobuf)

## Context

0058 named QID fields `qid` or `_qid`, and in protobuf they're strings. Nothing in the `.proto`
files said which entity a field holds: `added_with_qid` is a security key's, `qid` on a
`SupportRequest` a support session's, and a reader or an SDK generator had to read the comments.
Services mostly parsed QIDs into `awzy_model`'s typed IDs as requests arrived, but not all of them
did, or not at the boundary: a WebAuthn ceremony's QID was parsed deep in the key store, and a
break-glass report's bundle QID wasn't checked at all.

Protobuf has no custom scalar types. The options were a wrapper message (`message Qid { string
value = 1; }`), which changes every RPC's wire format and still doesn't say the entity, or a field
option on the string.

## Decision

- **A custom field option, `(awzy.authz.v1.qid)`**, in `awzy/authz/v1/qid.proto`, names the
  entity a field's QIDs are of, as their prefix: `string challenge_qid = 1
  [(awzy.authz.v1.qid) = "challenge"];`. The wire format doesn't change.
- **Every `qid`, `*_qid`, and `*_qids` field carries it.** `tools/ci/check-proto-qids.mjs`, run by
  `pnpm lint`, checks that each does, that the entity is one of those in
  `libs/model/src/qid.rs`, that a field named for an entity (`decision_qid`) names that one, and
  that nothing else carries the option.
- **A service parses a request's QIDs into typed IDs at its gRPC boundary**, and answers
  `INVALID_ARGUMENT` naming the entity expected. Code past the boundary takes `ChallengeId`,
  `CeremonyId`, and so on, never a string.

## Consequences

- The `.proto` files say what each ID is, for readers and for SDK generators that can read
  options.
- The option is checked against the typed IDs in Rust, so a new entity is added in one place,
  the `qids!` list.
- QIDs are still strings on the wire, so a client in another language can still send the wrong
  entity's QID; the service refuses it.

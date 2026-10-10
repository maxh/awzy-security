# 0058. Fields holding QIDs are named `_qid` on the wire

- Status: Accepted
- Date: 2026-10-10
- Amends: [0013](0013-qids-and-utc-timestamps.md) (how QID fields are named), and makes a one-time
  exception to [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (no edits to `v1`)

## Context

0013 made every awzy ID a QID wherever it leaves a service, but said nothing about field names.
They all ended up as `_id`, and so did IDs that aren't QIDs: `person_id` and `answerer_id` are
the client IdP's IDs, `credential_id` is a security key's bytes, `signing_key_id` is a key
fingerprint. A reader of `CheckResponse` or a GraphQL type can't tell which `_id` is one of
awzy's typed IDs and which came from somewhere else, and the difference matters: a QID parses
into one entity type, an external ID is opaque.

0045 made `authz/libs/proto` a public contract: a breaking change means a `v2` package, and CI's
`buf breaking` holds `v1` to that. Renaming a field breaks generated code and JSON field names,
though not the binary wire format, since field numbers don't change. No SDK or other generated
client exists outside this repository yet, so this is the cheapest the rename will ever be.

## Decision

- On every API surface (protobuf, GraphQL, and JSON over HTTP, including MCP tool arguments), a
  field holding an awzy QID is named for that: `decision_qid`, `challenge_qid`, `policy_bundle_qid`
  in protobuf and JSON; `decisionQid`, `challengeQid` in GraphQL. A message's or document's own
  QID is `qid`.
- IDs from the client's systems keep `_id`: `person_id`, `answerer_id`, `EntityRef.id`, service
  account IDs. So do IDs that aren't QIDs at all (`credential_id`, `signing_key_id`).
- GraphQL object types keep `id`. Relay identifies records by it and `node(id:)` is part of the
  spec, so the convention stops there. Inputs and fields that refer to another object use
  `…Qid`.
- Not renamed: storage (Postgres keeps native UUID columns, 0013), the decision log's columns and
  Kafka records, log fields, and configuration names (`AWZY_INSTALL_ID`, Terraform's
  `install_id`).
- **One-time exception to 0045:** the existing `v1` fields are renamed in place, not in a `v2`.
  CI's `buf breaking` check fails on the pull request that makes this change, as it should, and
  passes again once it's on `main`. From then on 0045 holds as written.
- The heartbeat from each install's awzy-reporter (0026) sends `install_qid`. The corp portal
  still reads `install_id`, because installs upgrade after the corp portal does.

Rejected: only naming new fields `_qid`. Two conventions side by side would leave the question
this record answers open for every existing field.

## Consequences

- Every enforcement point and client in this repository changes in the same commit. Nothing
  outside it is affected, since nothing outside it is generated from these files yet.
- Binary gRPC between an old and a new build still works (same field numbers). JSON from the
  PDP's AuthZEN endpoint changes shape: `qid` and `challenge_qid` instead of `id` and
  `challenge_id`.
- The corp portal can drop the `install_id` alias once every install reports `install_qid`.

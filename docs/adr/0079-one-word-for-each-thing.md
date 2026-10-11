# 0079. One word for each thing: principal, check, enforcement point, install

- Status: Accepted
- Date: 2026-10-10
- Amends: [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (a third one-time
  exception to its rule against changing `v1`, after 0058 and 0062) and
  [0074](0074-a-human-is-named-human-everywhere.md) (extends its rule to these words); an exception
  to [0072](0072-deploys-are-zero-downtime.md) (renames in one step, before the first install, as
  0076 was)

## Context

0074 settled `human`, and the switch from "accessor" to "principal" followed. The same thing
still had several names elsewhere:

- Who a check is about was a *principal* in Cedar, the docs, and the PDP's snapshot, but a
  *subject* on the wire (`CheckRequest.subject`), in the decision and event logs
  (`subject_type`, `subject_id`), in awzy-challenge's and awzy-context's tables, and in about 390
  places in the code.
- One question to the PDP was a *check* in the docs and in protobuf (`CheckRequest`), but an
  `AccessRequest` in the code.
- The software that asks it was an *enforcement point* in most places and a *PEP* in a few.
- One copy of awzy was an *install*, but sometimes a *deployment*.
- A configuration name still said `PEOPLE` (`AWZY_CHALLENGE_PEOPLE`), and the values of
  `sensitivity` were called tiers without anything saying so.

No client runs awzy yet, so nothing outside this repository depends on any of these names.

## Decision

- **Principal.** Who a check is about is the *principal*, in every name: protobuf fields
  (`CheckRequest.principal`, `Challenge.principal`, `Containment.principal`, and the rest),
  GraphQL, Rust, storage columns (`principal_type`, `principal_id`) in Postgres and ClickHouse,
  and the logs' JSON. Three other *subjects* keep their names, because they're other things: an
  `Actor`'s `subject` (who acted, which can be one of awzy's own services or a break-glass key),
  a token's `sub` and RFC 8693's `subject_token` (and `awzy_identity::Principal`'s `subject`, the
  entity a validated token names), and the Google user the Drive connector acts as.
- **Check.** One question to the PDP is a *check*: `awzy_model::Check`, as protobuf's
  `CheckRequest` already says.
- **Enforcement point**, never *PEP*, outside the platform plan's one definition of the term.
- **Install** for one copy of awzy in a client's cloud. *Deployment* keeps its other meanings: a
  Kubernetes Deployment, a deploy, and one environment awzy deploys to, an install or the vendor
  side (`deployment.env`, `tools/deployment-env`), which no other word covers.
- **Configuration** names say what the callers do: `AWZY_CHALLENGE_ANSWERERS`, not
  `AWZY_CHALLENGE_PEOPLE`.
- **Sensitivity tier**: the four values of `sensitivity`. Names that hold one say `tier`
  (`fail_open_tiers`).
- **Done in one step.** The protobuf fields are renamed in `v1`, field numbers unchanged, so the
  binary wire format is the same; `buf breaking` fails on the pull request and passes once it's
  on `main`. Postgres columns are renamed by migration, and ClickHouse's decision and event logs
  are rebuilt with the new columns (ClickHouse can't rename a column in a table's sort key) and
  their rows copied across.
- **Kept that way:** a CI check (`tools/ci/check-names.sh`) fails on `person`, `people`, or
  `accessor` in a name, and on *PEP* in the docs.

## Consequences

- One word for each of these things, from the policy language to the storage columns.
- Decision and event rows are copied once into the rebuilt tables; a large log takes a while to
  migrate, which no install has yet.
- From the first install on, a rename like this takes 0072's two releases.

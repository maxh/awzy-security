# 0004. Cedar for static policy, `@challenge` for challenges

- Status: Accepted
- Date: 2026-10-08

## Context

Static policy sets the floor in Beyond Zero. It needs to be fast, analyzable, and readable by
client security admins. Verdicts have three values (allow, deny, challenge), but policy engines
return two.

## Decision

- awzy-pdp embeds `cedar-policy`. The base schema and policies live in `policies/`.
- A forbid policy annotated `@challenge("<kind>")` turns a deny into a challenge of that kind.
- awzy-policy stores versioned bundles in Postgres, validates them strictly against the schema
  before storing a draft, and a publish supersedes the active bundle. The PDP swaps in a new
  bundle without restarting.
- The Cedar schema will be generated from awzy-context's attribute registry. Attributes added by
  connectors are optional, so policies test for them with `has`.

## Consequences

- Admins write Cedar directly for now. A higher-level policy builder is an open question.
- Accessor entities fit in an in-process snapshot. Resource entities (possibly millions) come from
  Valkey through a local LRU cache.

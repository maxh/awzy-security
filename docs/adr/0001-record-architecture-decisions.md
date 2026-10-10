# 0001. Record architecture decisions

- Status: Accepted
- Date: 2026-10-09

## Context

The first round of architecture decisions was made on 2026-10-08, partly in the architecture plan
artifact and partly in code. The plan describes the current design but not which parts were
argued over or what was rejected. That history is lost the next time the plan is edited.

## Decision

Keep architecture decision records in `docs/adr/`, one Markdown file per decision, in the
format of Michael Nygard's ADRs (status, context, decision, consequences). Records 0002 to 0014
write down the decisions from 2026-10-08.

## Consequences

- A change to a decision is a new record that supersedes the old one, so the history stays.
- The architecture plan can stay a description of the current design and link here for the reasons.

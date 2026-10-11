# 0039. A synthetic organization for the demo, development, and tests

- Status: Accepted
- Date: 2026-10-09
- Plan: [docs/plans/demo-plan.md](../plans/demo-plan.md)

## Context

awzy-context knows who people are (SCIM, service accounts) but nothing yet about what they
access or what they work on, and the connectors, intake, challenge, and reasoner services are
empty. Building that needs data: resources with owners and sensitivity, work assignments,
activity, and attacks to detect. awzy has no design partner data, the demo install may only hold
sample data (0032), and real sources need OAuth apps and accounts before the first line of
connector code is useful.

## Decision

- A generator produces a fictional company (Larkspur Capital) from a seed: people, groups,
  contractors, AI service accounts, resources with text and a true sensitivity, work items, and a
  month of activity, in a `demo` and a `scale` profile.
- The Beyond Zero paper's scenarios (the curious contractor, the rogue agent SalesGenie, the
  suddenly foolish administrator, the peer outlier) are injected into that activity, marked so
  detection can be measured.
- The data enters through the product's real paths: SCIM, the service-account API, a `synthetic`
  connector behind the `Connector` trait, MCP calls through awzy-gateway, and events to
  awzy-intake. Nothing writes to awzy's stores directly.
- It feeds the demo install, local development, the browser and scenario tests in CI, and the
  scale tests.

## Consequences

- The world model, PDP resource lookup, classification, and later challenges and reasoning can be
  built and shown before any design partner connects a source.
- Ground truth makes classification accuracy and detection rates measurable, and CI can fail a PR
  that breaks a scenario.
- Real sources' quirks and real data's messiness are not tested by this; the first real connector
  and monitor mode at a design partner are.
- The generator is product code to maintain as the model grows.

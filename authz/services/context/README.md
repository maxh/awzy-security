# awzy-context · world model

Owns the entity graph: principals, agents, teams, projects, resources, and edges between them.
Part of [awzy-authz](../../).

Runs LLM preprocessing to classify resource sensitivity and derive work assignments. Publishes
attribute changes so PDP snapshots stay current. People and groups come from the client's IdP
over SCIM ([ADR 0031](../../../docs/adr/0031-people-and-groups-from-the-idp-over-scim.md)), with
what the IdP says about each person (title, employee type, department, division, organization,
cost center) as attributes of their `Human` entity ([ADR 0116](../../../docs/adr/0116-people-carry-what-the-idp-says-about-them.md)), with their manager and, among the groups `AWZY_CONTEXT_TEAM_GROUPS` names, their team ([ADR 0117](../../../docs/adr/0117-a-persons-manager-and-team.md)), and service accounts are registered
here ([ADR 0033](../../../docs/adr/0033-service-account-registry-in-awzy-context.md)).

Resources (documents, folders, repositories, tickets) and work items (tickets, accounts, projects,
statements of work) are written by connectors through `/v1/resources` and `/v1/work-items`, one at
a time or in batches of up to 1,000 (demo plan step 2). `GET /v1/resources/{id}/entities` returns a
resource and its parent folders as the Cedar entities the PDP evaluates
([ADR 0044](../../../docs/adr/0044-pdp-loads-resources-from-awzy-context.md)); unclassified
resources get the default tier and `unclassified: true`. The same response lists `assignees`: who is on
open work that links the resource or a folder above it, which the PDP turns into
`context.assigned` ([ADR 0046](../../../docs/adr/0046-work-assignment-as-context-assigned.md)). IDs containing `/` are percent-encoded
in the path (`%2F`).

Resources without a label are classified in the background by
[awzy-classify](../../libs/classify/)'s rules, using content markings, then names and
folders. Policies see the higher of the label's tier and the classifier's
([ADR 0047](../../../docs/adr/0047-metadata-first-classification-rules.md)).

Every route except SCIM needs a service token (ADR 0055) from a caller the route's group
allows: *readers* (principals, resources, work items), *writers* (resource and work-item
writes), and *directory* (the directory and service accounts), each set by the chart
(`AWZY_CONTEXT_READERS`, `AWZY_CONTEXT_WRITERS`, `AWZY_CONTEXT_DIRECTORY`). SCIM keeps the IdP's
bearer token.

- Postgres with pgvector
- Change feed to Kafka topic `attributes.v1`

What's built so far: [platform plan](../../../docs/plans/platform-plan.md#built-so-far). Running it locally:
[DEVELOPMENT.md](../../../DEVELOPMENT.md#people-and-groups-from-the-idp-adr-0031).

# 0074. A human is `human` in every name, and several are `humans`

- Status: Accepted
- Date: 2026-10-10
- Amends: [0061](0061-ownership-and-sharing-in-the-resource-entity.md) (the sharing attributes'
  names) and [0062](0062-a-persons-id-on-the-wire-is-human-id.md) (extends its rename to every
  name)

## Context

The entity type is `Human` (0016), and 0062 named its ID `human_id`. Everything else still said
`person` or `people`: the sharing attributes `shared_people: Set<Human>` and `writer_people`, the
share kind connectors write (`"kind": "person"`), base policy IDs (`deactivated-people`), the
client portal's `activePeople`, and about sixty Rust identifiers. A reader had to know that
`people` meant `Human`, and the irregular plural broke the match between a name and its type.

## Decision

- **In every name, a human is `human`, and several are `humans`.** That covers code
  identifiers, wire fields, schema attributes, enum values, policy IDs, routes, and GraphQL. Never
  `person` or `people`. Prose (docs, comments, GraphQL descriptions, test names that read as
  sentences) stays plain English: "the person asking".
- **Renamed:**
  - Cedar: `shared_people` and `writer_people` are `shared_humans` and `writer_humans`.
  - Base policies: `humans-answer-their-challenges`, `deactivated-humans`,
    `crown-jewels-need-a-human-to-approve`.
  - awzy-context: the share kind `human`, `/v1/directory/humans`, and `humans` and
    `active_humans` in the directory summary.
  - The client portal's GraphQL: `humans`, `activeHumans`, `directoryHumans`.
  - Rust identifiers, such as `answering_human` and `Actor::for_human`.
- **Renamed without downtime (0072):** for one release, awzy-context writes both the old and new
  sharing attributes, and the PDP drops whichever its active bundle's schema doesn't declare, so a
  bundle published before this keeps working until the next publish. awzy-context accepts the
  `person` share kind from older connectors and reads it as `human`, as it does stored rows, and
  serves the old directory route and keys beside the new ones. The client portal keeps calling the
  old route, which every version serves, and reads either key. A later release removes the old
  names.
- **Not renamed:** storage columns (the security keys' `person_id`), which a rename would cost two
  releases for nothing a reader sees (0058's rule for storage), and decision log rows, which keep
  the policy IDs they were decided under.

## Consequences

- `shared_humans: Set<Human>` reads as what it is. Policies written against `shared_people` stop
  matching once their bundle is republished with the new schema; no client has written one yet.
- The Drive connector writes `human` shares, which awzy-context older than this rejects; during a
  rollout that only delays a sync round, and the round after succeeds.

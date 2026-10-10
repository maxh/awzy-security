# 0076. The rename to `human` finishes at once, since no install depends on the old names

- Status: Accepted
- Date: 2026-10-10
- Amends: [0074](0074-a-human-is-named-human-everywhere.md) and
  [0075](0075-storage-and-the-decision-log-say-human-too.md) (their transitions), as an exception
  to [0072](0072-deploys-are-zero-downtime.md) (two releases per rename)

## Context

0074 and 0075 renamed `person` and `people` to `human` and `humans`, and kept the old names
working for a release so that older and newer versions could run side by side (0072): sharing
attributes under both names, the `person` share kind, the old directory route and keys, a
`person_id` column kept in step by a trigger, notification recipients stored as `person:`, and a
map from renamed policy IDs to their new ones. Removing them was to wait for a later release.

No client runs awzy yet, so no install, bundle, or database depends on the old names.

## Decision

- Everything the transition kept is removed now, in one step: `shared_people` and
  `writer_people`, the `person` share kind, `/v1/directory/people` and its keys,
  `security_keys.person_id` and `webauthn_ceremonies.person_id` with their trigger and index (and
  `human_id` is `NOT NULL`), `person:` recipients, and the PDP's renamed-policy map.
- 0072's rule still holds for every rename from here on. This exception is for before the first
  install only.

## Consequences

- One name for each thing, everywhere, with no compatibility code to remove later.
- A development database or decision log from before 0074 may still hold old names (a bundle
  naming `deactivated-people`, a `person` share). Recreating it, or republishing the bundle,
  clears them.

# 0075. Storage and the decision log say `human` too

- Status: Accepted; amended by [0076](0076-the-human-rename-finishes-at-once.md) (finished at once)
- Date: 2026-10-10
- Amends: [0074](0074-a-human-is-named-human-everywhere.md) (which left storage columns and logged
  policy IDs alone) and [0058](0058-qid-fields-are-named-qid-on-the-wire.md) (storage keeps its
  names)

## Context

0074 renamed every `person` and `people` in names to `human` and `humans`, except in two places.
awzy-challenge's `person_id` columns (security keys and WebAuthn ceremonies) stayed, since a
column rename takes two releases (0072). And decisions already in the log kept the policy IDs
they were decided under (`deactivated-people`). Both leave the old word where people read it: in
the database and in the client portal's decision search and explanations. A bundle published
before the rename would also go on logging the old IDs until it's republished.

## Decision

- **`person_id` becomes `human_id`**, in two releases as 0072 requires. This one adds `human_id`,
  copies `person_id` into it, and adds a trigger that keeps the two equal on every write, so an
  older awzy-challenge (writing `person_id`) and this one (writing `human_id`) work side by side
  during a rollout. awzy-challenge reads and writes only `human_id`. The next release drops
  `person_id`, the trigger, and its index (the contract step), and makes `human_id` `NOT NULL`.
  Notification recipients stored as `person:<id>` follow the same path: read in either form now,
  written as `human:` from the next release.
- **Logged decisions use the current policy IDs.** A ClickHouse migration rewrites the three
  renamed base policies' IDs in every decision already logged, in both `policies` and
  `on_behalf_of_policies`. From now on the PDP reports a renamed policy under its current ID
  whichever bundle decided, and explain-decision finds a policy's source under either ID, since a
  signed bundle's text isn't rewritten.

Rejected: rewriting stored bundles' policy text. Bundles are signed (0021); rewriting one would
invalidate its signature, and break-glass restores bundles exactly as they were.

## Consequences

- The database, the decision log, and the client portal all say `human`.
- Renaming a base policy again means adding it to the PDP's `RENAMED_POLICIES` and a migration
  like this one.
- For one release, every security key write maintains two columns.

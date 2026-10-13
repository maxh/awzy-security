# 0123. What kind of data a resource holds, beside its tier

- Status: Accepted
- Date: 2026-10-13
- Builds on: [0047](0047-metadata-first-classification-rules.md) (classification rules first),
  [0061](0061-ownership-and-sharing-in-the-resource-entity.md) (resource attributes for policy)

## Context

The Beyond Zero paper's "what" is a resource's sensitivity *and* its type of data ("user data").
awzy has only the tier (`public` … `crown_jewels`). So a policy can't say "customer personal
data needs a work assignment" without also catching every confidential board paper, and the
synthetic company's customer export looks the same to policy as a budget (#357).

## Decision

- **A resource has `data_types`, a set of names, beside its tier.** The starter vocabulary is
  `customer_personal`, `employee_personal`, `financial`, `credentials`, `source_code`, `legal`,
  and `health`. In the Cedar schema it's `data_types?: Set<String>`, optional so a resource
  awzy-context doesn't know simply lacks it.
- **awzy-classify finds them with rules, from the same sources as the tier, in the same order:**
  what was sent with the resource and its labels, then phrases in the content excerpt, then its
  name and folders' names. Each phrase is listed only if it nearly always means that data is
  inside ("customer list", not "customer"). A repository holds `source_code`. As with tiers, a
  folder gets only what was sent or labeled. Each type keeps the most trusted reason for it,
  shown in the client portal.
- **They add up and are never removed.** Unlike a tier, which takes the highest finding, a
  resource holds every type any source names. The model step (#112) will only add to them, as it
  only raises tiers.
- **The client extends the vocabulary by sending types**, in a resource's
  `attributes.data_types`: lowercase names, theirs or the starter ones. Policies see the union of
  what was sent and what the classifier found. There's no registry of names; a policy names what
  it means.
- **They're stored with the classification** (`classified_data_types` in awzy-context, each with
  how it was found and why) and computed in the same background pass, never on the check path.
  Adding them reclassified every resource once.

Rejected: a data type per resource, like the tier. A client holdings export is customer personal
and financial at once, and a policy on either should see it. Rejected: deriving types from the
tier or the reverse. They answer different questions, and the point is to let policies tell
them apart.

## Consequences

- Policies can be written on what data is inside (`resource.data_types.contains(...)`), and dry
  run replays them like any attribute.
- Bundles published before the attribute existed still load: the PDP drops `data_types` for a
  schema that doesn't declare it.
- What rules can't find stays unfound until the model step; a missing type means "not found",
  not "absent", so a policy that needs certainty should challenge on `unclassified` too.

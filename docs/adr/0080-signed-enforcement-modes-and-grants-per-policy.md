# 0080. Enforcement modes are signed, nothing older replaces what a PDP applied, and a grant answers its own policies

- Status: Accepted
- Date: 2026-10-10
- Amends: [0021](0021-signed-policy-bundles.md) (signed bundles) and
  [0054](0054-challenges-and-containments-over-grpc.md) (challenge grants)

## Context

A security review found three ways around what the PDP enforces:

- **Installs never signed anything.** The chart set neither `AWZY_POLICY_SIGNING_KEY_FILE` nor
  `AWZY_POLICY_VERIFY_KEY_FILE`, so 0021's protection was off everywhere it mattered.
- **Signing bundles wasn't enough.** The enforcement mode sat unsigned in
  `enforcement_mode_changes`: inserting one `monitor` row there turned enforcement off. And a PDP
  accepted any signed bundle, so reactivating an old one in the database rolled policy back.
- **A passed challenge answered more than it was asked.** A grant covered its request for every
  `@challenge` policy of the kind it passed. A justification given for one policy also cleared a
  different policy asking for a justification, one that fired only later, when risk rose.

## Decision

- **Installs always sign.** Bootstrap generates a P-256 key and puts it in `awzy-policy-signing`
  (for awzy-policy) and its public half in `awzy-policy-verify` (for the PDPs), and the chart
  mounts both. Break-glass keys come from a bootstrap variable the same way.
- **Enforcement mode changes are signed** with the same key, over
  `awzy.enforcement.v1`, the scope, the mode, who set it, and when in whole Unix seconds
  (`awzy_signing::enforcement_payload`). A PDP with a verify key applies only a signed mode. A
  scope never switched still comes unsigned, as monitor, but only until the PDP has applied a
  signed change.
- **Nothing older replaces what a PDP applied.** A bundle's version has to go up (break-glass
  republishes as a new version, so it still does), and a mode change can't be older than the one
  in force. This holds for the life of a PDP process; a PDP that restarts accepts the current
  state as it finds it. Closing that gap needs state outside the database, such as a KMS-held
  counter, and waits for the KMS signer 0021 already plans.
- **A grant answers the policies that asked for it.** The PDP sends awzy-challenge the
  `@challenge` policies behind a verdict, each with its kind. The challenge records them, and a
  grant answers a policy only if it was opened for that policy and passed its kind. The PDP sets
  aside only the policies a grant answers. A policy that fires later, even one asking for the
  same kind, gets a challenge of its own. A caller that sends only kinds gets one stand-in policy
  per kind, which no Cedar policy matches, so the PDP sets nothing aside for it.

## Consequences

- Someone who can write to awzy-policy's database, but can't read the signing secret, can no
  longer change the policy or the mode a running PDP enforces.
- The signing key lives in Terraform state and a Kubernetes Secret until a KMS-backed signer
  replaces it.
- Two mode changes in the same second keep their order by ID, and the long-poll tells them apart
  by mode and author.
- A person may be asked again inside a grant's lifetime when a new policy fires. That's the point:
  their earlier answer was about something else.

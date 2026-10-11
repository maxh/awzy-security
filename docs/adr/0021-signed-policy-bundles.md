# 0021. Policy bundles are signed at publish; PDPs verify before loading

- Status: Accepted; amended by [0092](0092-awzy-policy-signs-with-the-installs-kms-key.md) (signs with the install's KMS key)
- Date: 2026-10-09

## Context

A PDP loads whatever bundle awzy-policy says is active. Anyone who can write to awzy-policy's
Postgres, or sit between it and the PDPs, could change what every PDP enforces without going
through publish, its validation, or its record of who published.

## Decision

- awzy-policy signs each bundle when it is published, inside the publish transaction, with ECDSA
  P-256 (`authz/signing`). Drafts aren't signed.
- The signature covers a fixed text payload: `awzy.policy.v1`, the bundle ID, the version, and
  the SHA-256 of the schema and of the policies, one per line. The ID and version are included so
  an old signed bundle can't be replayed as a newer one.
- A key ID, `sha256:<first 16 hex digits>` of the public key, travels with the signature so keys
  can rotate.
- A PDP given verify keys (`AWZY_POLICY_VERIFY_KEY_FILE`) refuses a bundle that is unsigned,
  signed by a key it doesn't trust, or altered. It keeps the bundle it has. A PDP that has never
  loaded a verified bundle stays not-ready, so enforcement points use their fallback.
- P-256 because every cloud KMS (AWS, GCP, Azure) and hardware security keys support it, so the
  file key used now can be swapped for KMS without changing the format.

## Consequences

- The private key is a file for now. A KMS-backed signer per cloud replaces it before a design
  partner install; the install's KMS key never leaves KMS.
- Without a signing key awzy-policy publishes unsigned bundles and logs a warning, and a PDP
  without verify keys accepts them. That's for local development only; installs set both.
- Rotating the key means trusting the new public key on every PDP before awzy-policy signs with it.
- The fallback bundles PEPs hold (0003) should be signed the same way.

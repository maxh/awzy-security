# 0092. awzy-policy signs with the install's KMS key

- Status: Accepted
- Date: 2026-10-11
- Amends: [0021](0021-signed-policy-bundles.md) (the key is a file) and
  [0080](0080-signed-enforcement-modes-and-grants-per-policy.md) (bootstrap generates it)

## Context

awzy-policy signs policy bundles, enforcement mode changes, and the outage fallback (0089), and
the PDPs and gateways apply only what verifies. The key was a PEM file bootstrap generated: it sat
in Terraform state and in a Kubernetes Secret, so anyone who could read either could sign policy
every PDP would accept. 0021 called for a KMS-backed signer before a design partner's install.

Every service in `awzy-system` ran as one Kubernetes service account, `awzy`, which Pod Identity
binds to one IAM role, so granting that role `kms:Sign` would let the PDP, the gateway, and the
rest sign policy too.

## Decision

- **The install module creates a KMS key for signing** (`ECC_NIST_P256`, `SIGN_VERIFY`) beside the
  envelope encryption key. The private key never leaves KMS.
- **awzy-policy runs as its own service account, `awzy-policy`**, with its own Pod Identity role,
  the only one allowed `kms:Sign` and `kms:GetPublicKey` on that key. It's part of the install
  contract's workload identities, like `awzy`, and awzy-challenge allows it, not the shared
  backend account, to report break-glass rollbacks.
- **awzy-policy signs through `PayloadSigner`.** `awzy_signing_aws::KmsSigner` asks KMS to sign
  each payload (RAW, `ECDSA_SHA_256`); KMS returns DER, and the key ID is the fingerprint of the
  public key KMS gives, so signatures look exactly as before and verifiers don't change. The AWS
  SDK is in its own crate, so only awzy-policy links it. A PEM key still works for local runs.
- **A signature KMS can't make stops the change.** It's made inside the transaction that stores
  it, so nothing unsigned is stored.
- **The install contract carries the key's ARN and public key**, and bootstrap writes only the
  public key to `awzy-policy-verify`. No private key exists outside KMS.
- **Verifiers trust one or more public keys.** To rotate: create a key, add its public key to
  `awzy-policy-verify` beside the old one, roll the PDPs and gateways, then point awzy-policy at
  the new key.

## Consequences

- Reading Terraform state or a Secret no longer lets anyone sign policy. Signing needs
  awzy-policy's role, and every signature is in CloudTrail.
- Each publish, mode change, and fallback change makes one KMS call, a few milliseconds.
- If KMS can't be reached, those changes fail until it can; what's already published keeps being
  enforced.
- GCP and Azure installs, when they come, need their own signer behind `PayloadSigner`.

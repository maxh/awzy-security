# 0059. Security keys answer verification challenges, checked by awzy's own narrow WebAuthn verifier

- Status: Accepted
- Date: 2026-10-10
- Implements part of: [0054](0054-challenges-and-containments-over-grpc.md) (the verification
  kind it left unbuilt)

## Context

Policies can already ask for `@challenge("verification")`: base policy `high-risk-verify` does it
whenever risk is above 80. Until now nothing could pass one, so a verification challenge just
expired. The platform plan means a security key: proof that the person holds a hardware key they
enrolled, which phishing and a stolen session can't fake.

WebAuthn is the standard. In Rust, the established relying-party library is webauthn-rs, which
uses OpenSSL for its cryptography with no pure-Rust option. This repository keeps C crypto out of
the hermetic Bazel build: it already chose ring over aws-lc for that reason. The only pure-Rust
relying-party crate found, webauthn_rp, is a single 0.1.0 release from 2023.

A key also has to be enrolled somehow. Trusting the IdP sign-in alone for every enrollment means
a stolen session can add the thief's key and pass every verification after.

## Decision

- **awzy-challenge checks WebAuthn itself, for one profile only:** ES256 credentials (ECDSA P-256
  with SHA-256) and "none" attestation (`authz/services/challenge/src/webauthn.rs`). It uses the
  `p256` and `sha2` crates the repository already uses to sign policy bundles, plus `ciborium` to
  read CBOR. It checks everything the signature covers:
  - the challenge awzy issued, and the type (`webauthn.create` or `webauthn.get`)
  - the origin, which must be one of the portal's, never a cross-origin frame
  - the RP ID hash
  - user presence
  - the signature
  - a counter that must move forward, unless a key never counts

  Anything else, such as RSA keys or attestation statements, is refused rather than half
  supported.
- **No attestation.** awzy asks for none and ignores any statement sent anyway. A key proves
  possession of what was enrolled, not who made it. An AAGUID allowlist can come later if a client
  needs one.
- **The RP ID is the client portal's hostname** (`AWZY_CHALLENGE_WEBAUTHN_RP_ID`), and the
  allowed origins are its URLs (`AWZY_CHALLENGE_WEBAUTHN_ORIGINS`). The chart sets both from
  `clientPortal.hostname`. Without them, security keys are off and the key RPCs say so.
- **Enrollment:** a person's first key is enrolled on the strength of their IdP sign-in.
  Adding another, or removing one, needs a fresh assertion from a key they already have, so a
  stolen session can't swap in the thief's key. An admin (`administer` on the challenges area) can
  reset a lost key, after which the person enrolls a first key again. Every key keeps the key
  that authorized it, and a removed key keeps who removed it.
- **Ceremonies** are server-side records in awzy-challenge's Postgres: 32 random bytes, one
  person, one purpose (`register`, `key_change`, `verify`), five minutes, single use. A ceremony is
  spent before its response is checked, so a failed attempt can't be retried against the same
  challenge. A `verify` ceremony is bound to one challenge, and the person must be the one it was
  sent to.
- **User verification is discouraged.** A security key here proves the key is present, not who
  holds it, so there's no PIN prompt.
- The portal runs the browser side on the Challenges page and passes binary fields as base64url.
  Only a person, never a service account, enrolls or uses keys.

Rejected:
- webauthn-rs with OpenSSL: it puts C crypto in the hermetic build.
- webauthn_rp: too young to trust with authentication.
- Passkeys as a sign-in method: the IdP signs people in, and awzy only asks for a second factor
  on a risky action.

## Consequences

- awzy maintains about 300 lines of security-critical verification code. Their unit tests build
  responses the way an authenticator does, with a software P-256 key, covering each check above.
  An end-to-end run in Chrome uses its virtual authenticator.
- Security keys that only offer RSA or EdDSA can't be enrolled. Current FIDO2 keys and platform
  authenticators all offer ES256.
- A person's first enrollment is only as strong as their IdP sign-in. The portal shows when each
  key was added, and admins can see and reset a person's keys.
- Biometric challenges and the LLM check of a justification are still to come.

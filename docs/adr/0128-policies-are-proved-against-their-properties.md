# 0128. Policies are proved against their properties with Cedar's symbolic compiler

- Status: Accepted
- Date: 2026-10-15

## Context

The P models (0126) check the protocols that carry policy to the PDPs, but not the policy. What
`policies/base.cedar` allows is checked by tests that each try one request: a deactivated person
reading a file, an agent switching enforcement. A policy that allows something on a request no
test tries, or a forbid that errors on one (Cedar skips a policy that errors, so a forbid fails
open), goes unnoticed.

Cedar has a symbolic compiler, SymCC, verified against Cedar's Lean model. It turns a policy set
and the schema into an SMT problem over every request the schema allows. A solver then proves a
property or returns a request that breaks it. The `cedar-policy-symcc` crate is released with
each Cedar version.

## Decision

- **Properties are written in Cedar,** in `policies/base.properties.cedar`: each is one permit
  that allows everything the property allows, such as anything but a deactivated person's
  requests. A policy set keeps a property when it allows nothing the property doesn't (SymCC's
  implication check). Each says whether it holds `outright` (nothing outside it is allowed
  without a challenge) or `after_challenges` (nothing outside it is allowed even with every
  challenge passed, the set without its `@challenge` forbids).
- **A property must rule something out:** the tests fail one that "permit everything" keeps, as
  #426 does for a P model that stops catching its bugs.
- **No base forbid may error** on any request the schema allows.
- **`awzy-policy-proofs`** (`authz/libs/policy-proofs`) holds the checks, so awzy-policy can run
  them on a bundle before signing it (#428), and its tests prove the base policies on every
  `bazel test`.
- **cvc5 1.3.1**, the solver SymCC supports, comes from its release's static build through
  `MODULE.bazel`, pinned by checksum for each platform the tests run on, so the proofs need no
  install and never skip.

## Consequences

- A change to `base.cedar` that breaks a property fails CI with a request that shows it.
  Changing a promise means changing its property in the same pull request, where a reviewer sees it.
- The properties restate some forbids. Those can only break if the forbid is edited or removed,
  or errors. The ones about permits, such as only the security team administering, also hold
  for whatever a client's bundle adds, which is what the check before signing is for.
- `cedar-policy-symcc` moves in lockstep with `cedar-policy`: upgrading Cedar waits for its
  release.
- The tests download about 40 MB of cvc5 the first time, and run on Linux (x86_64 and arm64)
  and arm64 Macs.

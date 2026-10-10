# 0009. No vendor access to client data; pull-based delivery

- Status: Accepted
- Date: 2026-10-08

## Context

awzy sits on the access path for a client's most sensitive data. A vendor with standing access,
or a compromised vendor release, would be the client's biggest risk.

## Decision

- awzy has no write access to an install, even in an emergency. Support sessions are read-only,
  time-boxed, scoped to named portal areas, approved by a client admin each time, and audited.
- The corp portal never holds client data. Installs push only health metrics, in a fixed,
  versioned schema with no free-form labels, which the client can inspect and turn off.
- Delivery is pull-based. The client runs the Terraform module, which installs Argo CD, and Argo CD
  pulls the Helm chart and images from a release channel (`stable`, `fast`, or a pinned version).
- Images are signed with cosign (keys in KMS, two-person release approval) and verified at
  admission by Kyverno. Releases soak in awzy staging, then `fast`, before `stable`.

## Consequences

- Repairs are run by the client's team from runbooks and repair Jobs. This is why managed data
  services are the default (0005).
- Diagnostics beyond health metrics come from a bundle a client admin generates, reviews, and sends.

# 0057. awzy's namespaces run only signed images from the release registry

- Status: Accepted
- Date: 2026-10-10

## Context

Kyverno checks the release key's signature on images from the release registry (ADR 0009), but
only on those: an image from anywhere else was admitted without a check. awzy's own namespaces
ran several. The chart pulled `postgres` and `clickhouse-server` from Docker Hub, and bootstrap's
Argo CD, ClickHouse operator, and cert-manager's HTTP-01 solver run in `awzy-ops` from Quay,
Docker Hub, and ECR Public. One of them was the operator's CRD hook on `bitnami/kubectl:latest`,
an unpinned tag Bitnami no longer maintains. So anyone who could create a pod there, or
compromise one of those tags upstream, could run unsigned code next to the PDP and its data.

## Decision

- **Only the release registry, only signed.** In `awzy-system`, `awzy-portal`, `awzy-data`, and
  `awzy-ops`, Kyverno refuses any container, init container, or ephemeral container whose image
  isn't under the install's `registry_prefix`, and those must carry the release key's signature
  as before.
- **Third-party images are mirrored and signed.** `deploy/release/third-party-images.txt` lists
  each one by upstream digest and a mirror tag. On every publish CI copies them to
  `awzy/third-party/<name>:<tag>` and signs them with the release key, so they reach installs
  through the same registry and pull token as awzy's own. Mirror tags never move: a new digest
  gets a new tag.
- **One list.** The ECR repositories (corp `build`) and bootstrap's image settings are read from
  the list. The chart names its images directly, and CI fails if one isn't in the list or comes
  from anywhere else.
- **Other namespaces are the client's.** Kyverno, External Secrets, Envoy Gateway, and
  cert-manager's controllers run in their own namespaces from upstream, as many clients already
  run them. The policy doesn't reach there.

## Consequences

- Signing a third-party image means awzy vouches that it's the upstream image it names. It
  doesn't mean awzy reviewed the code.
- Updating a component that bootstrap installs (`var.versions`) needs its new images mirrored
  first, or its pods are refused.
- A new third-party repository needs its ECR repository created (corp `build` Terraform) before
  CI can mirror into it.
- Docker Hub's pull limits no longer apply to installs.

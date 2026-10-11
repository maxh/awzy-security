# 0015. Cloud-agnostic install: per-cloud infrastructure, shared bootstrap

- Status: Accepted; amended by [0094](0094-installs-follow-their-channel-through-the-registry.md) (installs follow their channel through the registry)
- Date: 2026-10-09

## Context

0009 settled on pull-based delivery: the client runs a Terraform module that installs Argo CD,
and Argo CD pulls the Helm chart and images from a release channel. 0002 requires the same
product on AWS, Google Cloud, and Azure. The cluster, network, identities, and managed data
services can't be created the same way on every cloud. Everything that runs on top of the
cluster can.

## Decision

- Split the install into two Terraform layers:
  - `deploy/terraform/{aws,gcp,azure}/`: per-cloud infrastructure (network, cluster, bucket,
    keys, identities, managed Postgres, Kafka, and Valkey, registry mirror). Each module outputs
    the same `install_contract` object.
  - `deploy/terraform/bootstrap/`: cloud-agnostic, using only the `helm` and `kubernetes`
    providers. It installs Argo CD, Kyverno with the cosign verification policy, and the root
    Argo CD Application, passing `install_contract` in as Helm values.
- `install_contract` is the only interface between the layers: cloud name, data service
  endpoints, bucket, KMS key ID, workload-identity annotations per service account, storage
  class, registry prefix, and size profile. `bootstrap` never branches on the cloud; it passes the
  cloud name through so Helm values select the `libs/cloud` adapter.
- The umbrella Helm chart and the images are OCI artifacts in the vendor registry. The chart
  refers to images by digest.
- Release channels: a pinned install sets Argo CD's `targetRevision` to an exact chart version.
  `stable` and `fast` are small per-channel artifacts that name an exact chart version, and the
  release process moves them.
- The client-side registry mirror (ECR pull-through cache, an Artifact Registry remote repository,
  or ACR artifact cache) is per-cloud Terraform. The chart sees only a registry prefix.
- A client who won't let a vendor module create their cluster runs `bootstrap` alone against an
  existing cluster that passes the preflight check.

## Consequences

- Adding a cloud means writing one infrastructure module that produces `install_contract` and the
  `libs/cloud` adapters. `bootstrap`, the chart, and the release process don't change.
- Phase 0 must check how Argo CD handles moving OCI tags (or semver constraints) before the
  channel design relies on it.
- Installs need egress to the vendor registry, or a mirror seeded ahead of time.
- The Terraform state backend (S3, GCS, or azurerm) is the client's choice and stays per cloud.
- The deployment plan's AWS layout (`aws/{bootstrap,install,monitoring}`) predates this record and
  should move `bootstrap` to the shared `deploy/terraform/bootstrap/`.

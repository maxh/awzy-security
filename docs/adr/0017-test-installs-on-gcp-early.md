# 0017. Test installs on Google Cloud before a design partner needs it

- Status: Accepted, amended by [0018](0018-awzysecurity-domain-and-dns.md)
- Date: 2026-10-09
- Amends: [0002](0002-single-tenant-installs-in-client-clouds.md) (AWS only until a design partner needs another cloud)

## Context

0002 kept everything on AWS until a design partner asks for Google Cloud or Azure. 0015 split the
install into a per-cloud infrastructure module and a cloud-agnostic `bootstrap/`, with
`install_contract` between them. Until a second cloud runs that split, nothing tests that
`bootstrap/`, the Helm chart, and the release process are really cloud-agnostic. Finding that out
when a design partner is waiting would be expensive.

There is no awzy domain yet. Google Cloud only creates an Organization for a Cloud Identity or
Workspace domain, so a Gmail account gets standalone projects with no org policies.

## Decision

- Add a second example client on Google Cloud, installed exactly the way a real GCP client would
  install awzy. The vendor side stays on AWS (`awzy-corp`).
- Start after the AWS example install works by pull (deployment plan steps 1 to 4), because the
  chart and `bootstrap/` have to exist first.
- Terraform: `deploy/terraform/gcp/{state,install,monitoring}`, with `gcp/install` outputting the
  same `install_contract` as `aws/install`, and `examples/example-client-gcp` as the caller.
  `bootstrap/` is reused unchanged. Any GCP-specific change it needs is a bug in 0015's split.
- Services: GKE Standard, Cloud SQL for PostgreSQL, Managed Service for Apache Kafka, Memorystore
  for Valkey, GCS, Cloud KMS, Workload Identity Federation for GKE, Vertex AI, and an Artifact
  Registry remote repository as the registry mirror. Monitoring uses Google Cloud Managed Service
  for Prometheus and Cloud Trace.
- `libs/cloud` gets GCP adapters for KMS, the LLM provider, and workload identity. Object storage
  already goes through `object_store`.
- Until there is a domain, the example client is a standalone project under the Gmail account,
  with its own billing account. When a domain exists, create a Cloud Identity Free organization,
  move the project into it, and add org policies that a cautious client would use (the GCP
  counterpart of the SCPs on the AWS example client).
- The vendor registry must be pullable from any cloud with a per-install credential. ECR with a
  repository policy keyed on AWS account IDs only works for AWS installs, and ECR pull tokens
  expire every 12 hours, so a GCP mirror can't hold one. Which registry (or a small token issuer in
  front of ECR) is decided before the first GCP install, in its own record.

## Consequences

- The portability rules from 0002 are tested before Phase 5 instead of during it.
- More to build early: the GCP module and adapters move ahead of Phase 5, while the corp portal
  and the other Phase 5 work stay there.
- Cost: new GCP accounts get trial credit, but Managed Kafka and Cloud SQL cost money while idle,
  so the GCP install is destroyed between test runs, as on AWS. If trial quotas block GKE or
  Managed Kafka, the billing account is upgraded to paid (unused credit carries over).
- Without an organization, the GCP example client doesn't test org policies. That gap stays until
  there is a domain.
- The registry choice also changes the AWS example install, which currently pulls from ECR by
  account ID.

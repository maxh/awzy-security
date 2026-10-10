# 0002. Single-tenant installs in client clouds, AWS first

- Status: Accepted, amended by [0017](0017-test-installs-on-gcp-early.md) (GCP test installs start early)
- Date: 2026-10-08

## Context

awzy implements the Beyond Zero model (Valente and Zalewski, ACM 2026): a per-action, per-resource
verdict for every request from a human or an agent. To decide, it needs the client's most
sensitive context: who works on what, how sensitive each resource is, and what each accessor did
recently. Clients will not send that to a vendor's SaaS.

## Decision

- Every client gets its own install of awzy-authz and the client portal, in a Kubernetes cluster
  in the client's own cloud account, with its data stores in that account. Client data never leaves it.
- There is no multi-tenant SaaS version in the first year.
- Kubernetes is the shared platform, so the services are identical on AWS, Google Cloud, and
  Azure. Cloud-specific pieces (object storage, KMS, workload identity, LLM provider, private
  networking, managed data services) sit behind small traits in `libs/cloud` and per-cloud
  Terraform modules. Services never branch on the cloud.
- Development and the first design partners are on AWS only. GCP and Azure installs are built
  when a design partner needs one (Phase 5).

## Consequences

- The portability rules (cloud traits, portable Kubernetes features only) hold from day one but
  are not tested until the second cloud arrives.
- awzy cannot log in to an install, which drives 0005 and 0009.
- Cost per install is a floor on price. The `small` profile's cost is measured in Phase 0.

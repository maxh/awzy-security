# 0035. awzy's own services run on ECS Fargate and deploy from CI

- Status: Accepted
- Date: 2026-10-09

## Context

Client installs run on Kubernetes because the product has to be the same on AWS, Google Cloud,
and Azure (0002, 0015). awzy's own side is different: the corp portal and the release registry
proxy (0028) run only in awzy's AWS organization, awzy operates them itself, and they are two
small stateless services in front of one Postgres. A cluster of awzy's own would add an EKS
control plane, node groups, ingress, and upgrades for two containers.

## Decision

- The corp portal (`corp.awzysecurity.com`) and the registry proxy (`registry.awzysecurity.com`)
  run as ECS Fargate services behind one Application Load Balancer, with Aurora PostgreSQL
  Serverless v2 (pausing when idle). Terraform: `deploy/terraform/corp/corp-portal`.
- **CI deploys every commit on `main`**: after publishing images, it assumes `awzy-ci-deploy`
  (only `main` can, like the publish role), registers task definition revisions with that
  commit's images, updates both services, and fails if ECS's circuit breaker rolls one back
  (`tools/ci/deploy-corp.sh`).
- **Terraform owns the infrastructure but not the running image**: the services ignore
  `task_definition` changes, so an infrastructure apply never moves them back to an older build.
- They run in `awzy-build` for now, because the new organization may only have two member
  accounts (0023). They move to `awzy-corp-portal` once AWS raises the limit: the same Terraform
  with another provider profile.

## Consequences

- No Kubernetes to run on awzy's side. If awzy's own services grow (for example an LLM-backed
  support tool), this record is revisited rather than ECS stretched.
- Every merge to `main` is a production deploy of the corp portal and registry. That makes
  PR review and CI (0037) the gate, and the circuit breaker the safety net.
- Deploys need nothing on a laptop. Infrastructure changes still run `tofu apply` by hand.
- Build and serving share one account until the move, which mixes the blast radius of CI's
  publish role and the services; acceptable while there are no real clients.

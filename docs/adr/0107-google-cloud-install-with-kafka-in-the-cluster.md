# 0107. A Google Cloud install, with Kafka in the cluster

- Status: Accepted
- Date: 2026-10-11
- Amends: the platform plan's "AWS only until a design partner needs GCP or Azure"

## Context

The platform plan built AWS installs only, and left Google Cloud and Azure until a design partner
needed them (Phase 5). awzy's case is that the same product runs single-tenant in any client's
cloud; one cloud doesn't show that, and portability problems found late are expensive. Bootstrap,
the chart, and the services are already cloud-agnostic. What's per cloud is the cloud layer, which
outputs `install_contract`, and a few adapters behind `libs/cloud`.

The plan's default for Kafka on Google Cloud is its Managed Service for Apache Kafka. That service
requires SASL, which awzy's Kafka client and ClickHouse's Kafka engine don't speak, and its
smallest cluster costs about as much as the rest of a small install.

## Decision

- **`deploy/terraform/gcp/install`**, the counterpart of `aws/install`, outputs the same contract:
  a VPC with Cloud NAT; zonal GKE Standard with Workload Identity Federation and Dataplane V2; Cloud
  SQL for PostgreSQL on a private IP, its credentials in Secret Manager; a GCS bucket and Cloud KMS
  keys (data, and the P-256 policy signing key); and IAM granted directly to the Kubernetes service
  accounts, with no Google service accounts or keys.
- **Kafka runs in the cluster with Strimzi** (the plan's in-cluster option), KRaft, keeping seven
  days. The managed service, with SASL, follows when a client needs it.
- **No Memorystore:** awzy uses no Valkey yet.
- **`examples/example-client-gcp`** runs it in its own project, as the second demo
  (`gcp.demo.awzysecurity.com`, 0106).
- **awzy-policy signs with Cloud KMS** (`awzy-signing-gcp`) on Google Cloud.

## Consequences

- Not yet on Google Cloud: ClickHouse backups (0105; GCS speaks the S3 API only with HMAC keys),
  Awzy Bot and investigations (Vertex AI), connector credentials set in the client portal (it
  writes to AWS Secrets Manager only), and managed monitoring.
- In-cluster Kafka is the install's to run: its disk and upgrades are Strimzi's, not a managed
  service's. A single node for the small profile means a node's loss stops decisions reaching
  ClickHouse until it's back; the PDPs keep deciding.

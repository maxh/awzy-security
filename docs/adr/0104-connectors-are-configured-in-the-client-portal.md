# 0104. Connectors are configured in the client portal

- Status: Proposed
- Date: 2026-10-11

## Context

The Google Drive and Jira connectors (ADR 0069, 0070) are configured at install time. Their
settings (Drive's subject and domain, Jira's site, account, and projects) are chart values, and
their credentials (Drive's service account key, Jira's API token) are Terraform variables that
bootstrap writes into Kubernetes Secrets. Changing either takes someone with the install's
Terraform and a sync. The platform plan has a client admin do it in the client portal, which
today only shows each connector's status (#141).

Settings are not secret; credentials are. The question is where credentials live once the portal
writes them. The options were:

- **Kubernetes Secrets the portal writes.** Simplest, but RBAC can't limit `update` on Secrets to
  a few names for `create`, and the portal would hold write access to Secrets in the system
  namespace, beside every other credential there.
- **Encrypted in Postgres**, with a KMS key. One design on every cloud, but the credentials then
  sit in a database that awzy-context and the portal read, and whatever decrypts them needs the
  key.
- **The cloud's secrets manager.** The install already keeps its Postgres password there and syncs
  it into the cluster with External Secrets Operator. Access is per secret, through workload
  identity, and every write is in the cloud's audit log.

## Decision

- **Each connector's credential is a secret in the cloud's secrets manager**:
  `<install>/connectors/drive` and `<install>/connectors/jira` in AWS Secrets Manager, encrypted
  with the install's KMS key. The install's Terraform creates them, empty.
- **The client portal writes credentials and never reads them.** Its workload role may
  `PutSecretValue` on those secrets and nothing else in Secrets Manager. The write goes through a
  `SecretWriter` trait in `libs/cloud`, with an AWS adapter; Google Secret Manager and Azure Key
  Vault adapters come with those clouds.
- **External Secrets Operator syncs each credential into the Kubernetes Secret the connector
  already mounts** (`awzy-connectors-drive`, `awzy-connectors-jira`), refreshing every minute.
  Its role may read those two secrets. Bootstrap stops writing the Secrets and drops its `drive`
  and `jira` variables.
- **Settings live in awzy-context**, in a `connector_settings` table, set through its API. The
  chart's `connectors.drive` and `connectors.jira` values go away.
- **awzy-connectors reads its settings from awzy-context and its credentials from the mounted
  files at the start of each round**, so a change takes effect at the next round without a
  restart. It's always deployed, one replica, and idles while nothing is configured.
- **The portal's `configureConnector` and `disableConnector`** are for those who may administer
  the enforcement area, and policy asks for a security key first, as it does for switching
  enforcement: a connector decides what awzy can read. The Connectors page shows each connector's
  settings and when its credential was last set, never the credential.

## Consequences

- A client admin connects or changes Drive and Jira from the portal; the install's Terraform is
  no longer involved.
- The portal's workload identity can overwrite two secrets, and nothing else. Neither the portal
  nor awzy-context can read a credential back.
- A new credential reaches the connectors within about a minute (External Secrets' refresh), and
  takes effect at their next round.
- awzy-connectors depends on awzy-context for its settings, as it already does for writing what
  it reads.
- GCP and Azure installs need their secret-writer adapters and External Secrets stores before
  they can configure connectors in the portal.

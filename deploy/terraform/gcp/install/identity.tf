# Workload Identity Federation for GKE: IAM grants each Kubernetes service account directly
# (local.workload_principal), so there are no Google service accounts or keys. The chart and
# bootstrap create these service accounts; their names are part of install_contract.

locals {
  workloads = {
    clickhouse = { namespace = "awzy-data", service_account = "clickhouse" }
    services   = { namespace = "awzy-system", service_account = "awzy" }
    portal     = { namespace = "awzy-portal", service_account = "awzy" }
    # Apart from the other services, so only awzy-policy may sign policy.
    policy           = { namespace = "awzy-system", service_account = "awzy-policy" }
    external_secrets = { namespace = "external-secrets", service_account = "external-secrets" }
  }
  principal = { for k, w in local.workloads : k => "${local.workload_principal}/ns/${w.namespace}/sa/${w.service_account}" }
}

# awzy-policy alone signs with the policy key, and reads its public key.
resource "google_kms_crypto_key_iam_member" "policy_signs" {
  crypto_key_id = google_kms_crypto_key.policy_signing.id
  role          = "roles/cloudkms.signerVerifier"
  member        = local.principal.policy
  # The identity pool exists once the cluster does.
  depends_on = [google_container_cluster.this]
}

# External Secrets reads the Postgres credentials and the connectors' into the cluster.
resource "google_secret_manager_secret_iam_member" "external_secrets" {
  for_each  = merge({ postgres = google_secret_manager_secret.postgres.id }, { for k, s in google_secret_manager_secret.connector : "connector-${k}" => s.id })
  secret_id = each.value
  role      = "roles/secretmanager.secretAccessor"
  member    = local.principal.external_secrets
  # The identity pool exists once the cluster does.
  depends_on = [google_container_cluster.this]
}

# awzy's services: connector raw data in the bucket.
resource "google_storage_bucket_iam_member" "services" {
  bucket = google_storage_bucket.data.name
  role   = "roles/storage.objectUser"
  member = local.principal.services
  # The identity pool exists once the cluster does.
  depends_on = [google_container_cluster.this]
}

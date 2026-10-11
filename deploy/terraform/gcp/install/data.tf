# --- Keys: one for data at rest, and the one awzy-policy signs policy with (ADR 0092).

resource "google_kms_key_ring" "this" {
  project  = var.project
  name     = var.name
  location = var.region
}

resource "google_kms_crypto_key" "data" {
  name            = "${var.name}-data"
  key_ring        = google_kms_key_ring.this.id
  purpose         = "ENCRYPT_DECRYPT"
  rotation_period = "7776000s" # 90 days
}

# P-256, as every awzy signature is: the PDPs and gateways verify against its public key.
resource "google_kms_crypto_key" "policy_signing" {
  name     = "${var.name}-policy-signing"
  key_ring = google_kms_key_ring.this.id
  purpose  = "ASYMMETRIC_SIGN"
  version_template {
    algorithm = "EC_SIGN_P256_SHA256"
  }
}

data "google_kms_crypto_key_version" "policy_signing" {
  crypto_key = google_kms_crypto_key.policy_signing.id
}

# --- The bucket: connector raw data, and later ClickHouse's cold tier and backups.

data "google_storage_project_service_account" "gcs" {
  project = var.project
}

resource "google_kms_crypto_key_iam_member" "gcs" {
  crypto_key_id = google_kms_crypto_key.data.id
  role          = "roles/cloudkms.cryptoKeyEncrypterDecrypter"
  member        = "serviceAccount:${data.google_storage_project_service_account.gcs.email_address}"
}

resource "google_storage_bucket" "data" {
  project                     = var.project
  name                        = "${var.name}-data-${local.project_number}"
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = !var.deletion_protection
  encryption {
    default_kms_key_name = google_kms_crypto_key.data.id
  }
  depends_on = [google_kms_crypto_key_iam_member.gcs]
}

# --- Postgres: Cloud SQL on a private IP in the VPC. Its credentials go to Secret Manager in the
# shape RDS keeps them ({"username", "password"}), which the chart's ExternalSecret reads.

locals {
  postgres_tier = {
    small    = "db-custom-1-3840"
    standard = "db-custom-2-7680"
    large    = "db-custom-4-15360"
  }[var.size_profile]
}

resource "google_sql_database_instance" "postgres" {
  project             = var.project
  name                = var.name
  region              = var.region
  database_version    = "POSTGRES_17"
  deletion_protection = var.deletion_protection
  settings {
    tier              = local.postgres_tier
    edition           = "ENTERPRISE"
    availability_type = var.size_profile == "small" ? "ZONAL" : "REGIONAL"
    disk_type         = "PD_SSD"
    disk_size         = 20
    disk_autoresize   = true
    ip_configuration {
      ipv4_enabled                                  = false
      private_network                               = google_compute_network.this.id
      enable_private_path_for_google_cloud_services = true
      ssl_mode                                      = "ENCRYPTED_ONLY"
    }
    backup_configuration {
      enabled                        = true
      point_in_time_recovery_enabled = true
      transaction_log_retention_days = 7
      backup_retention_settings {
        retained_backups = 7
      }
    }
  }
  depends_on = [google_service_networking_connection.services]
}

resource "google_sql_database" "awzy" {
  project  = var.project
  name     = "awzy"
  instance = google_sql_database_instance.postgres.name
}

resource "random_password" "postgres" {
  length  = 32
  special = false
}

resource "google_sql_user" "awzy" {
  project  = var.project
  name     = "awzy"
  instance = google_sql_database_instance.postgres.name
  password = random_password.postgres.result
}

resource "google_secret_manager_secret" "postgres" {
  project   = var.project
  secret_id = "${var.name}-postgres"
  replication {
    auto {}
  }
}

resource "google_secret_manager_secret_version" "postgres" {
  secret      = google_secret_manager_secret.postgres.id
  secret_data = jsonencode({ username = google_sql_user.awzy.name, password = random_password.postgres.result })
}

# The one interface to deploy/terraform/bootstrap (ADR 0015), in the shape every cloud's install
# module outputs.
output "install_contract" {
  value = {
    cloud        = "gcp"
    region       = var.region
    size_profile = var.size_profile
    cluster = {
      name     = google_container_cluster.this.name
      endpoint = google_container_cluster.this.endpoint
    }
    postgres = {
      host     = google_sql_database_instance.postgres.private_ip_address
      port     = 5432
      database = google_sql_database.awzy.name
      # The Secret Manager secret holding {"username", "password"}, by the name External Secrets
      # reads it with.
      secret_arn = google_secret_manager_secret.postgres.secret_id
    }
    kafka = {
      # In the cluster (kafka.tf), so plaintext: it never leaves the cluster's network.
      bootstrap_brokers = "awzy-kafka-bootstrap.${local.kafka_namespace}.svc:9092"
      tls               = false
    }
    object_store = {
      bucket = google_storage_bucket.data.name
      # No S3-compatible URL: GCS speaks the S3 API only with HMAC keys, so ClickHouse's backups
      # (ADR 0105) are off on Google Cloud until it can use Workload Identity for them.
      url = ""
    }
    kms_key_id = google_kms_crypto_key.data.id
    # awzy-policy signs with the key version, by its name (projects/…/cryptoKeyVersions/N; the
    # data source's id is a //cloudkms.googleapis.com URI); the PDPs and gateways trust the public
    # key (ADR 0092).
    policy_signing = {
      kms_key        = data.google_kms_crypto_key_version.policy_signing.name
      public_key_pem = data.google_kms_crypto_key_version.policy_signing.public_key[0].pem
    }
    # Workload Identity Federation grants these principals directly; no annotations needed.
    workload_identity = local.principal
    # bootstrap creates both from this data, so it never needs to know the cloud.
    storage_class = {
      name        = "pd-balanced"
      provisioner = "pd.csi.storage.gke.io"
      parameters  = { type = "pd-balanced" }
    }
    # The Gateway's Service gets GKE's external passthrough load balancer as is.
    load_balancer = {
      service_annotations = {}
    }
    # External Secrets provider block (ClusterSecretStore spec.provider); credentials come from
    # Workload Identity on the external-secrets service account.
    secret_store = {
      gcpsm = {
        projectID = var.project
      }
    }
    registry_prefix = var.registry_prefix
    # Where the client portal writes each connector's credential (ADR 0104).
    connector_secrets = { for k, s in google_secret_manager_secret.connector : k => s.secret_id }
  }
}

# For the caller's kubernetes and helm providers: where the cluster's API is, and its CA.
output "cluster" {
  value = {
    name                   = google_container_cluster.this.name
    endpoint               = "https://${google_container_cluster.this.endpoint}"
    cluster_ca_certificate = base64decode(google_container_cluster.this.master_auth[0].cluster_ca_certificate)
  }
}

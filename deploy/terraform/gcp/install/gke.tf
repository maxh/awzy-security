# GKE Standard, zonal: one cluster whose management fee GKE's free tier covers. Workload Identity
# Federation lets IAM grant awzy's Kubernetes service accounts directly (identity.tf), and
# Dataplane V2 enforces the chart's NetworkPolicies. Private nodes, public endpoint limited to
# gke_master_authorized_cidrs.

locals {
  # Per size profile: node machine type and count. Only small is sized so far: the AWS small
  # profile's two t3.large nodes ran it full, so these have twice the CPU.
  nodes = {
    small    = { machine_type = "e2-standard-4", count = 2 }
    standard = { machine_type = "e2-standard-8", count = 3 }
    large    = { machine_type = "e2-standard-16", count = 3 }
  }[var.size_profile]
}

resource "google_container_cluster" "this" {
  project             = var.project
  name                = var.name
  location            = var.zone
  network             = google_compute_network.this.id
  subnetwork          = google_compute_subnetwork.nodes.id
  deletion_protection = var.deletion_protection

  # The node pool below replaces the default one.
  remove_default_node_pool = true
  initial_node_count       = 1

  release_channel {
    channel = "REGULAR"
  }
  datapath_provider = "ADVANCED_DATAPATH"
  workload_identity_config {
    workload_pool = "${var.project}.svc.id.goog"
  }
  ip_allocation_policy {
    cluster_secondary_range_name  = "pods"
    services_secondary_range_name = "services"
  }
  private_cluster_config {
    enable_private_nodes    = true
    enable_private_endpoint = false
    master_ipv4_cidr_block  = "172.16.0.0/28"
  }
  master_authorized_networks_config {
    dynamic "cidr_blocks" {
      for_each = var.gke_master_authorized_cidrs
      content {
        cidr_block = cidr_blocks.value
      }
    }
  }
  gateway_api_config {
    # Envoy Gateway (bootstrap) brings its own Gateway API CRDs.
    channel = "CHANNEL_DISABLED"
  }
}

resource "google_container_node_pool" "general" {
  project    = var.project
  name       = "general"
  location   = var.zone
  cluster    = google_container_cluster.this.name
  node_count = local.nodes.count
  node_config {
    machine_type = local.nodes.machine_type
    disk_type    = "pd-balanced"
    disk_size_gb = 50
    oauth_scopes = ["https://www.googleapis.com/auth/cloud-platform"]
    workload_metadata_config {
      mode = "GKE_METADATA"
    }
    shielded_instance_config {
      enable_secure_boot = true
    }
  }
  management {
    auto_repair  = true
    auto_upgrade = true
  }
}

# Monitoring for the example client's Google Cloud install (ADR 0125): gcp/monitoring on the
# cluster examples/example-client-gcp/install made, applied after bootstrap.

terraform {
  required_version = ">= 1.10"
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 8.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.38"
    }
  }

  # bucket comes from TF_CLI_ARGS_init (deployment.env).
  backend "gcs" {
    prefix = "example-client-gcp/monitoring"
  }
}

data "terraform_remote_state" "install" {
  backend = "gcs"
  config = {
    bucket = var.state_bucket
    prefix = "example-client-gcp/install"
  }
}

locals {
  contract = data.terraform_remote_state.install.outputs.install_contract
}

provider "google" {
  project = var.project
  region  = var.region
}

data "google_client_config" "this" {}

data "google_container_cluster" "this" {
  name     = local.contract.cluster.name
  location = var.zone
}

provider "kubernetes" {
  host                   = "https://${data.google_container_cluster.this.endpoint}"
  cluster_ca_certificate = base64decode(data.google_container_cluster.this.master_auth[0].cluster_ca_certificate)
  token                  = data.google_client_config.this.access_token
}

module "monitoring" {
  source       = "../../../gcp/monitoring"
  name         = "awzy"
  alert_emails = var.alert_emails
}

# The example client's Google Cloud install: deploy/terraform/gcp/install in its own project.

terraform {
  required_version = ">= 1.10"
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 8.0"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 3.0"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.6"
    }
  }

  # bucket comes from TF_CLI_ARGS_init (deployment.env).
  backend "gcs" {
    prefix = "example-client-gcp/install"
  }
}

provider "google" {
  project = var.project
  region  = var.region
}

data "google_client_config" "this" {}

# Kafka runs in the cluster this makes (gcp/install/kafka.tf).
provider "helm" {
  kubernetes = {
    host                   = module.install.cluster.endpoint
    cluster_ca_certificate = module.install.cluster.cluster_ca_certificate
    token                  = data.google_client_config.this.access_token
  }
}

module "install" {
  source = "../../../gcp/install"

  project         = var.project
  region          = var.region
  zone            = var.zone
  registry_prefix = var.registry_prefix
  # A test install, destroyed between uses.
  deletion_protection = false
}

output "install_contract" {
  value = module.install.install_contract
}

output "cluster" {
  value = { name = module.install.cluster.name, endpoint = module.install.cluster.endpoint }
}

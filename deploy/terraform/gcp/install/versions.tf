# What a client runs on Google Cloud to create everything awzy needs below the cluster contract
# (ADR 0015): the counterpart of deploy/terraform/aws/install. Outputs the same install_contract
# for deploy/terraform/bootstrap. Nothing here knows about awzy's accounts: awzy's registry
# arrives as an input.
#
# The caller configures the google provider for the install's project, and the kubernetes and
# helm providers for the cluster this makes (for Kafka, which runs in it: kafka.tf).

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
}

data "google_project" "this" {
  project_id = var.project
}

locals {
  project_number = data.google_project.this.number
  # GKE Workload Identity Federation: IAM grants a Kubernetes service account directly, as this
  # principal, with no Google service account in between.
  workload_principal = "principal://iam.googleapis.com/projects/${local.project_number}/locations/global/workloadIdentityPools/${var.project}.svc.id.goog/subject"
}

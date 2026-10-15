# Monitoring for a Google Cloud install (ADR 0125): Google Cloud Managed Service for Prometheus
# scrapes every awzy service's metrics port, and Cloud Monitoring evaluates the repository's alert
# rules as PromQL alert policies that email whoever is listed. Nothing more runs in the cluster:
# GKE runs Managed Prometheus's collectors already.

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
}

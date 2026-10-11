# Monitoring for an AWS install (ADR 0096): Amazon Managed Service for Prometheus, fed by its
# managed scraper from every awzy pod's /metrics, with the repository's alert rules routed to an
# SNS topic; an OpenTelemetry collector that sends traces to X-Ray (ADR 0097, traces.tf); and an
# Amazon Managed Grafana workspace that reads all three and CloudWatch. The dashboards
# go in with deploy/terraform/grafana, which works with any Grafana.

terraform {
  required_version = ">= 1.10"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.38"
    }
    time = {
      source  = "hashicorp/time"
      version = "~> 0.13"
    }
  }
}

data "aws_region" "current" {}

data "aws_eks_cluster" "this" {
  name = var.network.cluster_name
}

# Monitoring for the example client's install (ADR 0096): aws/monitoring on the cluster
# examples/example-client/install created. Then examples/example-client/grafana loads the
# dashboards.

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
  }

  # bucket, region, and profile come from TF_CLI_ARGS_init (deployment.env).
  backend "s3" {
    key          = "example-client/monitoring/terraform.tfstate"
    use_lockfile = true
  }
}

data "terraform_remote_state" "install" {
  backend = "s3"
  config = {
    bucket  = var.state_bucket
    key     = "example-client/install/terraform.tfstate"
    region  = var.region
    profile = var.management_profile
  }
}

locals {
  network = data.terraform_remote_state.install.outputs.monitoring
}

provider "aws" {
  region              = var.region
  profile             = var.install_profile
  allowed_account_ids = [var.install_account_id]
}

data "aws_eks_cluster" "this" {
  name = local.network.cluster_name
}

provider "kubernetes" {
  host                   = data.aws_eks_cluster.this.endpoint
  cluster_ca_certificate = base64decode(data.aws_eks_cluster.this.certificate_authority[0].data)
  exec {
    api_version = "client.authentication.k8s.io/v1beta1"
    command     = "aws"
    args = concat(
      ["eks", "get-token", "--cluster-name", local.network.cluster_name, "--region", var.region],
      var.install_profile == null ? [] : ["--profile", var.install_profile],
    )
  }
}

module "monitoring" {
  source = "../../../aws/monitoring"

  name                   = local.network.cluster_name
  network                = local.network
  alert_emails           = var.alert_emails
  grafana_admin_user_ids = var.grafana_admin_user_ids
}

output "grafana" {
  value     = module.monitoring.grafana
  sensitive = true
}

# awzy's dashboards in the example install's Managed Grafana (ADR 0096). Runs after
# examples/example-client/monitoring, whose output has the workspace and a token for it.

terraform {
  required_version = ">= 1.10"
  required_providers {
    grafana = {
      source  = "grafana/grafana"
      version = ">= 3.0"
    }
  }

  # bucket, region, and profile come from TF_CLI_ARGS_init (deployment.env).
  backend "s3" {
    key          = "example-client/grafana/terraform.tfstate"
    use_lockfile = true
  }
}

data "terraform_remote_state" "monitoring" {
  backend = "s3"
  config = {
    bucket  = var.state_bucket
    key     = "example-client/monitoring/terraform.tfstate"
    region  = var.region
    profile = var.management_profile
  }
}

locals {
  grafana = data.terraform_remote_state.monitoring.outputs.grafana
}

provider "grafana" {
  url  = local.grafana.url
  auth = local.grafana.token
}

# Managed Grafana's data sources sign their requests with the workspace's own role.
module "grafana" {
  source = "../../../grafana"

  prometheus_url = local.grafana.prometheus_url
  prometheus_json_data = {
    sigV4Auth     = true
    sigV4AuthType = "ec2_iam_role"
    sigV4Region   = local.grafana.region
  }
  cloudwatch_json_data = {
    authType      = "ec2_iam_role"
    defaultRegion = local.grafana.region
  }
  xray_json_data = {
    authType      = "ec2_iam_role"
    defaultRegion = local.grafana.region
  }
}

# Bootstrap for the example client's install: runs the cloud-agnostic bootstrap module on the
# EKS cluster that examples/example-client/install created. Only the provider wiring here is
# AWS-specific; bootstrap itself never is (ADR 0015).

terraform {
  required_version = ">= 1.10"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 3.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.38"
    }
  }

  # bucket, region, and profile come from TF_CLI_ARGS_init (deployment.env).
  backend "s3" {
    key          = "example-client/bootstrap/terraform.tfstate"
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
  contract = data.terraform_remote_state.install.outputs.install_contract
}

provider "aws" {
  region              = var.region
  profile             = var.install_profile
  allowed_account_ids = [var.install_account_id]
}

# The example client keeps what awzy gave it at registration in its own Secrets Manager:
# install ID, reporting token (ADR 0026), and registry pull token (ADR 0028).
provider "aws" {
  alias               = "management"
  region              = var.region
  profile             = var.management_profile
  allowed_account_ids = [var.management_account_id]
}

data "aws_secretsmanager_secret_version" "install_credentials" {
  provider  = aws.management
  secret_id = "awzy/install-credentials"
}

locals {
  install_credentials = jsondecode(data.aws_secretsmanager_secret_version.install_credentials.secret_string)
}

data "aws_eks_cluster" "this" {
  name = local.contract.cluster.name
}

locals {
  kube = {
    host                   = data.aws_eks_cluster.this.endpoint
    cluster_ca_certificate = base64decode(data.aws_eks_cluster.this.certificate_authority[0].data)
    exec = {
      api_version = "client.authentication.k8s.io/v1beta1"
      command     = "aws"
      args = concat(
        ["eks", "get-token", "--cluster-name", local.contract.cluster.name, "--region", var.region],
        var.install_profile == null ? [] : ["--profile", var.install_profile],
      )
    }
  }
}

provider "kubernetes" {
  host                   = local.kube.host
  cluster_ca_certificate = local.kube.cluster_ca_certificate
  exec {
    api_version = local.kube.exec.api_version
    command     = local.kube.exec.command
    args        = local.kube.exec.args
  }
}

provider "helm" {
  kubernetes = {
    host                   = local.kube.host
    cluster_ca_certificate = local.kube.cluster_ca_certificate
    exec                   = local.kube.exec
  }
}

module "bootstrap" {
  source = "../../../bootstrap"

  install_contract  = local.contract
  cosign_public_key = file("${path.module}/../../../../release/cosign.pub")
  # The fast channel, which this install is registered with (ADR 0094), unless pinned here.
  chart_version = var.chart_version

  registry_credentials = {
    username = local.install_credentials.install_id
    password = local.install_credentials.pull_token
  }
  reporter = {
    install_id = local.install_credentials.install_id
    token      = local.install_credentials.reporting_token
  }

  # The demo hostname (ADR 0027). Its Cloudflare CNAME points at the Gateway's load
  # balancer: `kubectl -n awzy-ops get gateway awzy` shows the address once it exists.
  ingress = {
    hostname   = var.demo_hostname
    acme_email = var.acme_email
  }

  # The demo signs in against the dev IdP (ADR 0032): no real accounts, sample data only.
  demo_idp = true
  # And the synthetic company (ADR 0039, docs/plans/demo-plan.md) to show it with, working through
  # awzy-gateway (in-cluster only) so the portal has decisions.
  demo_data     = true
  demo_activity = true
  mcp_gateway   = { upstreams = [] }
}

variable "chart_version" {
  description = "awzy chart versions to follow: by default whatever the install's channel names; or an exact 0.1.0-g<sha> from CI"
  type        = string
  default     = ">=0.0.0-0"
}


output "scim" {
  value     = module.bootstrap.scim
  sensitive = true
}

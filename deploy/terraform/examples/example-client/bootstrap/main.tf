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

# Traces go to the collector monitoring/ runs, once it's applied (ADR 0097).
data "terraform_remote_state" "monitoring" {
  backend = "s3"
  config = {
    bucket  = var.state_bucket
    key     = "example-client/monitoring/terraform.tfstate"
    region  = var.region
    profile = var.management_profile
  }
  defaults = { otlp_endpoint = null }
}

locals {
  contract      = data.terraform_remote_state.install.outputs.install_contract
  otlp_endpoint = data.terraform_remote_state.monitoring.outputs.otlp_endpoint
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
  # The corp portal's support key, once it exists (ADR 0095).
  support_public_key = fileexists("${path.module}/../../../../release/support.pub") ? file("${path.module}/../../../../release/support.pub") : null
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

  # The demo hostname (ADR 0027, 0106), and older ones that redirect to it. Each has a Cloudflare
  # CNAME to the Gateway's load balancer: `kubectl -n awzy-ops get gateway awzy` shows the address.
  ingress = {
    hostname           = var.demo_hostname
    acme_email         = var.acme_email
    redirect_hostnames = var.demo_redirect_hostnames
  }

  # The demo signs in against the dev IdP (ADR 0032): no real accounts, sample data only.
  demo_idp = true
  # And the synthetic company (ADR 0039, docs/plans/demo-plan.md) to show it with, working through
  # awzy-gateway (in-cluster only) so the portal has decisions.
  demo_data     = true
  demo_activity = true
  # awzy-intake, in-cluster only, so apps built on the client libraries can report what they
  # showed (the partner's path in tools/demo).
  intake      = {}
  mcp_gateway = { upstreams = [] }
  # awzy-reasoner investigates the people the scenarios flag (ADR 0071). This account can't use
  # Bedrock's models yet, so with Gemini on the Google Cloud demo's Vertex AI, through an API key
  # bound to a service account that may only call models (ADR 0114).
  investigations  = true
  llm_connections = { vertex = { kind = "vertex", project = var.vertex_project, api_key_env = "AWZY_LLM_KEY_VERTEX" } }
  llm_api_keys    = { AWZY_LLM_KEY_VERTEX = var.vertex_api_key }
  llm_routing = {
    models = [{ name = "gemini", connection = "vertex", spec = { api = "gemini", model = "gemini-2.5-pro" } }]
    routes = { investigations = { targets = [{ model = "gemini" }] } }
  }
  tracing = local.otlp_endpoint == null ? null : { otlp_endpoint = local.otlp_endpoint }
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

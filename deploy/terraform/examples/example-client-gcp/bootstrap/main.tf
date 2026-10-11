# Bootstrap for the example client's Google Cloud install: the same cloud-agnostic bootstrap
# module as the AWS example's, on the GKE cluster examples/example-client-gcp/install made. Only
# the provider wiring here is Google Cloud's (ADR 0015).

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
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.38"
    }
  }

  # bucket comes from TF_CLI_ARGS_init (deployment.env).
  backend "gcs" {
    prefix = "example-client-gcp/bootstrap"
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

# What awzy gave the install at registration, kept in the project's Secret Manager as JSON:
# install ID, reporting token (ADR 0026), and registry pull token (ADR 0028).
data "google_secret_manager_secret_version" "install_credentials" {
  secret = "awzy-install-credentials"
}

locals {
  install_credentials = jsondecode(data.google_secret_manager_secret_version.install_credentials.secret_data)
}

data "google_client_config" "this" {}

data "google_container_cluster" "this" {
  name     = local.contract.cluster.name
  location = var.zone
}

locals {
  kube = {
    host                   = "https://${data.google_container_cluster.this.endpoint}"
    cluster_ca_certificate = base64decode(data.google_container_cluster.this.master_auth[0].cluster_ca_certificate)
    token                  = data.google_client_config.this.access_token
  }
}

provider "kubernetes" {
  host                   = local.kube.host
  cluster_ca_certificate = local.kube.cluster_ca_certificate
  token                  = local.kube.token
}

provider "helm" {
  kubernetes = local.kube
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

  # The Google Cloud demo's hostname (ADR 0106). Its Cloudflare CNAME points at the Gateway's load
  # balancer: `kubectl -n awzy-ops get gateway awzy` shows the address once it exists.
  ingress = {
    hostname   = var.demo_hostname
    acme_email = var.acme_email
  }

  # As the AWS demo: the dev IdP (ADR 0032), and the synthetic company working through awzy-gateway
  # (ADR 0039), so the portal has decisions.
  # GKE runs its own metrics-server.
  metrics_server = false

  demo_idp      = true
  demo_data     = true
  demo_activity = true
  mcp_gateway   = { upstreams = [] }
}

output "scim" {
  value     = module.bootstrap.scim
  sensitive = true
}

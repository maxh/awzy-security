# The corp portal at https://corp.<domain> (ADR 0027): ECS Fargate behind an HTTPS load
# balancer, with Aurora PostgreSQL Serverless v2. Holds install records and heartbeats, never
# client data (ADR 0009). Sign-in is username and password, for demos and testing only
# (ADR 0025).
#
# Runs in the build account until a corp-portal account exists (account quota); moving it means
# pointing the provider below at that account and applying again.

terraform {
  required_version = ">= 1.10"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 5.0"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.6"
    }
  }

  # State in the build account, where this module works. OpenTofu reads variables here
  # (deployment.env); plain Terraform can't.
  backend "s3" {
    bucket       = var.build_state_bucket
    key          = "corp/corp-portal/terraform.tfstate"
    region       = var.region
    profile      = var.build_profile
    use_lockfile = true
  }
}

provider "aws" {
  region              = var.region
  profile             = var.build_profile
  allowed_account_ids = [var.build_account_id]
}

# DNS for var.domain is on Cloudflare (ADR 0018).
provider "cloudflare" {
  api_token = trimspace(file(pathexpand(var.cloudflare_token_file)))
}

# The images the services were created with. Since then CI deploys every commit on main
# (tools/ci/deploy-corp.sh), and Terraform ignores which image is running.
variable "image_tag" {
  description = "Commit SHA of the awzy-corp-portal image the service was created with"
  type        = string
  default     = "514f799121eafab85cac595592fef3100b1b0a8a"
}

variable "registry_image_tag" {
  description = "Commit SHA of the awzy-registry image the service was created with"
  type        = string
  default     = "514f799121eafab85cac595592fef3100b1b0a8a"
}

locals {
  name     = "awzy-corp-portal"
  hostname = "corp.${var.domain}"
  # The release registry proxy (ADR 0028) shares this load balancer and database.
  registry_hostname = "registry.${var.domain}"
  cf_zone_id        = var.cloudflare_zone_id
  # ECR in the build account, where CI publishes.
  ecr_host     = "${var.build_account_id}.dkr.ecr.${var.region}.amazonaws.com"
  image        = "${local.ecr_host}/awzy/awzy-corp-portal:${var.image_tag}"
  accounts_dir = "/config"
}

data "aws_region" "current" {}

# What a client runs on AWS to create everything awzy needs below the cluster contract
# (ADR 0015). Outputs install_contract for deploy/terraform/bootstrap. Nothing here knows
# about awzy's accounts: awzy's registry and signing key arrive as inputs.

terraform {
  required_version = ">= 1.10"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
  }
}

data "aws_region" "current" {}
data "aws_caller_identity" "current" {}

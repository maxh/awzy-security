# The S3 buckets that hold Terraform state for the vendor side (deploy/terraform/corp/*). Each
# account keeps the state of what runs in it: the management account for state/ and org/, the
# build account for build/ and corp-portal/. So routine changes in the build account never need
# the management account.
#
# Bootstrap: applied once with local state, then `tofu init -migrate-state` moves this
# module's own state into the bucket (see ../README.md).

terraform {
  required_version = ">= 1.10"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
  }

  # OpenTofu reads variables here (deployment.env); plain Terraform can't.
  backend "s3" {
    bucket       = var.state_bucket
    key          = "corp/state/terraform.tfstate"
    region       = var.region
    profile      = var.management_profile
    use_lockfile = true
  }
}

provider "aws" {
  region              = var.region
  profile             = var.management_profile
  allowed_account_ids = [var.management_account_id]
}

provider "aws" {
  alias               = "build"
  region              = var.region
  profile             = var.build_profile
  allowed_account_ids = [var.build_account_id]
}

module "management" {
  source = "./bucket"
  name   = var.state_bucket
}

module "build" {
  source    = "./bucket"
  name      = var.build_state_bucket
  providers = { aws = aws.build }
}

# The management bucket's resources before they moved into ./bucket.
moved {
  from = aws_s3_bucket.state
  to   = module.management.aws_s3_bucket.this
}
moved {
  from = aws_s3_bucket_versioning.state
  to   = module.management.aws_s3_bucket_versioning.this
}
moved {
  from = aws_s3_bucket_server_side_encryption_configuration.state
  to   = module.management.aws_s3_bucket_server_side_encryption_configuration.this
}
moved {
  from = aws_s3_bucket_public_access_block.state
  to   = module.management.aws_s3_bucket_public_access_block.this
}
moved {
  from = aws_s3_bucket_ownership_controls.state
  to   = module.management.aws_s3_bucket_ownership_controls.this
}
moved {
  from = aws_s3_bucket_policy.state
  to   = module.management.aws_s3_bucket_policy.this
}

output "bucket" {
  value = module.management.id
}

output "build_bucket" {
  value = module.build.id
}

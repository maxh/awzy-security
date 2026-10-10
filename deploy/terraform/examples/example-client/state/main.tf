# Terraform state for the example client (examples/example-client/*), in the example client's
# management account; awzy never holds a client's state. Calls the same aws/state module a
# client runs.
#
# Bootstrap: applied once with local state, then `tofu init -migrate-state` moved this
# root's own state into the bucket (same steps as deploy/terraform/corp/README.md).

terraform {
  required_version = ">= 1.10"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
  }

  # bucket, region, and profile come from TF_CLI_ARGS_init (deployment.env).
  backend "s3" {
    key          = "example-client/state/terraform.tfstate"
    use_lockfile = true
  }
}

provider "aws" {
  region              = var.region
  profile             = var.management_profile
  allowed_account_ids = [var.management_account_id]
}

module "state" {
  source      = "../../../aws/state"
  bucket_name = var.state_bucket
}

moved {
  from = aws_s3_bucket.state
  to   = module.state.aws_s3_bucket.state
}

moved {
  from = aws_s3_bucket_versioning.state
  to   = module.state.aws_s3_bucket_versioning.state
}

moved {
  from = aws_s3_bucket_server_side_encryption_configuration.state
  to   = module.state.aws_s3_bucket_server_side_encryption_configuration.state
}

moved {
  from = aws_s3_bucket_public_access_block.state
  to   = module.state.aws_s3_bucket_public_access_block.state
}

moved {
  from = aws_s3_bucket_ownership_controls.state
  to   = module.state.aws_s3_bucket_ownership_controls.state
}

moved {
  from = aws_s3_bucket_policy.state
  to   = module.state.aws_s3_bucket_policy.state
}

output "bucket" {
  value = module.state.bucket
}

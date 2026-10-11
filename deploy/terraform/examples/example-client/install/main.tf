# The example client's install of awzy, in its install account. Calls the same
# aws/install module a real client runs. Costs roughly $250+/month while it exists, so destroy
# it between test runs (docs/plans/deployment-plan.md, cost control).

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
    key          = "example-client/install/terraform.tfstate"
    use_lockfile = true
  }
}

# Two profiles: the backend reads state in the management account, the provider works in the
# install account.
provider "aws" {
  region              = var.region
  profile             = var.install_profile
  allowed_account_ids = [var.install_account_id]
}

module "install" {
  source = "../../../aws/install"

  azs                 = ["${var.region}a", "${var.region}b"]
  registry_prefix     = var.registry_prefix
  deletion_protection = false # test install, destroyed between runs
}

output "install_contract" {
  value = module.install.install_contract
}

# Member accounts of the example client's organization. Runs as the example
# client's management account. A real client manages its own accounts; this stands in for that.

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
    key          = "example-client/org/terraform.tfstate"
    use_lockfile = true
  }
}

provider "aws" {
  region              = var.region
  profile             = var.management_profile
  allowed_account_ids = [var.management_account_id]
}

resource "aws_organizations_organization" "this" {
  feature_set = "ALL"
  # sso: IAM Identity Center, enabled in the console (the API can't create an organization
  # instance in the management account).
  aws_service_access_principals = ["sso.amazonaws.com"]
  enabled_policy_types          = ["SERVICE_CONTROL_POLICY"]

  lifecycle {
    prevent_destroy = true
  }
}

# Guardrails a cautious client commonly sets, so the install is tested against them
# (docs/plans/deployment-plan.md, step 3). SCPs don't apply to the management account.
resource "aws_organizations_policy" "region_lock" {
  name        = "deny-regions-except-${var.region}"
  description = "Deny everything outside ${var.region}, except global services"
  content = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Sid    = "DenyOtherRegions"
      Effect = "Deny"
      NotAction = [
        "account:*", "budgets:*", "ce:*", "cloudfront:*", "health:*", "iam:*",
        "organizations:*", "route53:*", "route53domains:*", "sso:*", "sts:*", "support:*",
        "trustedadvisor:*", "waf:*", "wafv2:*",
      ]
      Resource = "*"
      Condition = {
        StringNotEquals = { "aws:RequestedRegion" = [var.region] }
        # Bedrock serves Claude through cross-region inference profiles: a call to one in this
        # region runs the model in another, which is allowed only through the profile.
        ArnNotLike = { "bedrock:InferenceProfileArn" = "arn:aws:bedrock:*:*:inference-profile/us.*" }
      }
    }]
  })
}

resource "aws_organizations_policy" "no_iam_user_keys" {
  name        = "deny-iam-users-and-access-keys"
  description = "People sign in through IAM Identity Center; workloads use roles"
  content = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Sid      = "DenyIamUsersAndKeys"
      Effect   = "Deny"
      Action   = ["iam:CreateUser", "iam:CreateAccessKey", "iam:CreateLoginProfile"]
      Resource = "*"
    }]
  })
}

resource "aws_organizations_policy_attachment" "root" {
  for_each = {
    region_lock      = aws_organizations_policy.region_lock.id
    no_iam_user_keys = aws_organizations_policy.no_iam_user_keys.id
  }
  policy_id = each.value
  target_id = aws_organizations_organization.this.roots[0].id
}

resource "aws_organizations_account" "install" {
  name              = var.install_account.name
  email             = var.install_account.email
  close_on_deletion = false

  lifecycle {
    prevent_destroy = true
  }
}

output "install_account_id" {
  value = aws_organizations_account.install.id
}

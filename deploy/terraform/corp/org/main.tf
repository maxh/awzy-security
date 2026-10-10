# Member accounts of the vendor organization. Runs as the management account.
#
# AWS created the organization when advanced features were activated (see ADR 0014), so the
# organization itself, Identity Center, and the AWS-managed SCPs/RCPs are not managed here yet.

terraform {
  required_version = ">= 1.10"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
  }

  # State in the management account, where this module works. OpenTofu reads variables here
  # (deployment.env); plain Terraform can't.
  backend "s3" {
    bucket       = var.state_bucket
    key          = "corp/org/terraform.tfstate"
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

# Member accounts come from var.member_accounts (deployment.env): each needs a root email of
# its own. An account that already exists is adopted with an `import` block naming its ID.

resource "aws_organizations_account" "this" {
  for_each = var.member_accounts

  name              = each.value.name
  email             = each.value.email
  close_on_deletion = false

  lifecycle {
    prevent_destroy = true
    # No Organizations API reads these back, so an imported account always shows a diff.
    ignore_changes = [role_name, iam_user_access_to_billing]
  }
}

output "account_ids" {
  value = { for k, a in aws_organizations_account.this : k => a.id }
}

# --- Resource control policy. AWS created it on activation (ADR 0023). Every listed service,
# ECR included, denies principals outside this organization: installs pull release images
# through the registry proxy (ADR 0028), which runs inside it.

locals {
  rcp_org_only_services = [
    "aoss:*", "appconfig:*", "appstream:*", "autoscaling:*", "codebuild:*", "codecommit:*",
    "cognito-identity:*", "cognito-idp:*", "comprehend:*", "comprehendmedical:*", "dax:*",
    "dynamodb:*", "ecr:*", "health:*", "kinesisvideo:*", "kms:*", "logs:*", "s3:*",
    "secretsmanager:*", "sqs:*", "sts:*", "support:*", "textract:*", "transcribe:*",
    "translate:*",
  ]
}

resource "aws_organizations_policy" "rcp" {
  name        = "ManagedAccountResourceControlPolicy"
  description = "ManagedAccountResourceControlPolicy"
  type        = "RESOURCE_CONTROL_POLICY"
  content = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid       = "DenyAnyoneOutsideMyOrgAndAWS"
        Effect    = "Deny"
        Principal = "*"
        Action    = local.rcp_org_only_services
        Resource  = "*"
        Condition = {
          StringNotEqualsIfExists = { "aws:PrincipalOrgID" = "$${aws:ResourceOrgID}" }
          Null                    = { "aws:PrincipalARN" = "false" }
          BoolIfExists            = { "aws:PrincipalIsAWSService" = "false" }
        }
      },
      {
        Sid       = "DenyAWSWhenItsNotMyOrgsSourceAccount"
        Effect    = "Deny"
        Principal = "*"
        Action    = local.rcp_org_only_services
        Resource  = "*"
        Condition = {
          Null            = { "aws:SourceAccount" = "false" }
          StringNotEquals = { "aws:SourceOrgID" = "$${aws:ResourceOrgID}" }
        }
      },
    ]
  })

  lifecycle {
    prevent_destroy = true
  }
}

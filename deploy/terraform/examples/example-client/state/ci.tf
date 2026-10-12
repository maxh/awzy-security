# What CI's roles need in the management account (ADR 0115): the Terraform state, and the
# install credentials bootstrap reads. Read-only for plans from pull requests and main; read and
# write for the example-aws environment's applies. The install account's roles do the rest
# (install/ci.tf). Applied by hand, like the rest of this module.

resource "aws_iam_openid_connect_provider" "github" {
  url            = "https://token.actions.githubusercontent.com"
  client_id_list = ["sts.amazonaws.com"]
}

locals {
  ci_subjects = {
    plan      = ["repo:${var.github_oidc_subject_repo}:ref:refs/heads/main", "repo:${var.github_oidc_subject_repo}:pull_request"]
    terraform = ["repo:${var.github_oidc_subject_repo}:environment:example-aws"]
  }
}

data "aws_iam_policy_document" "ci_trust" {
  for_each = local.ci_subjects
  statement {
    actions = ["sts:AssumeRoleWithWebIdentity"]
    principals {
      type        = "Federated"
      identifiers = [aws_iam_openid_connect_provider.github.arn]
    }
    condition {
      test     = "StringEquals"
      variable = "token.actions.githubusercontent.com:aud"
      values   = ["sts.amazonaws.com"]
    }
    condition {
      test     = "StringEquals"
      variable = "token.actions.githubusercontent.com:sub"
      values   = each.value
    }
  }
}

data "aws_iam_policy_document" "ci" {
  for_each = local.ci_subjects
  statement {
    actions   = ["s3:ListBucket"]
    resources = ["arn:aws:s3:::${var.state_bucket}"]
  }
  statement {
    actions   = each.key == "plan" ? ["s3:GetObject"] : ["s3:GetObject", "s3:PutObject", "s3:DeleteObject"]
    resources = ["arn:aws:s3:::${var.state_bucket}/example-client/*"]
  }
  statement {
    actions   = ["secretsmanager:GetSecretValue"]
    resources = ["arn:aws:secretsmanager:${var.region}:${var.management_account_id}:secret:awzy/install-credentials-*"]
  }
}

resource "aws_iam_role" "ci" {
  for_each             = local.ci_subjects
  name                 = "awzy-example-ci-state-${each.key}"
  assume_role_policy   = data.aws_iam_policy_document.ci_trust[each.key].json
  max_session_duration = 3600
}

resource "aws_iam_role_policy" "ci" {
  for_each = local.ci_subjects
  name     = "state"
  role     = aws_iam_role.ci[each.key].id
  policy   = data.aws_iam_policy_document.ci[each.key].json
}

# For CI's environment variables AWZY_CORP_EXAMPLE_AWS_STATE_PLAN_ROLE_ARN and _STATE_TERRAFORM_ROLE_ARN.
output "ci_role_arns" {
  value = { for k, r in aws_iam_role.ci : k => r.arn }
}

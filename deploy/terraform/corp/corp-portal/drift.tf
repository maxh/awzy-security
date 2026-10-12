# The role CI plans the corp modules as, without refreshing (ADR 0115): for a pull request's list
# of changes (.github/workflows/terraform-corp.yml), and for the drift check, which says when
# merged Terraform hasn't been applied (.github/workflows/terraform-drift.yml). Read-only, and only
# what a plan without a refresh reads: the state, and the few things the modules look up.
# Workflows on main and on pull requests can assume it; pull requests from forks get no token.

data "aws_iam_policy_document" "ci_plan_trust" {
  statement {
    actions = ["sts:AssumeRoleWithWebIdentity"]
    principals {
      type        = "Federated"
      identifiers = [data.aws_iam_openid_connect_provider.github.arn]
    }
    condition {
      test     = "StringEquals"
      variable = "token.actions.githubusercontent.com:aud"
      values   = ["sts.amazonaws.com"]
    }
    condition {
      test     = "StringEquals"
      variable = "token.actions.githubusercontent.com:sub"
      values = [
        "repo:${var.github_oidc_subject_repo}:ref:refs/heads/main",
        "repo:${var.github_oidc_subject_repo}:pull_request",
      ]
    }
  }
}

resource "aws_iam_role" "ci_plan" {
  name                 = "awzy-ci-plan"
  assume_role_policy   = data.aws_iam_policy_document.ci_plan_trust.json
  max_session_duration = 3600
}

data "aws_iam_policy_document" "ci_plan" {
  statement {
    actions   = ["s3:ListBucket"]
    resources = ["arn:aws:s3:::${var.build_state_bucket}"]
  }
  statement {
    actions   = ["s3:GetObject"]
    resources = ["arn:aws:s3:::${var.build_state_bucket}/corp/*"]
  }
  # The data sources the corp modules read.
  statement {
    actions = [
      "cloudfront:GetCachePolicy",
      "cloudfront:GetResponseHeadersPolicy",
      "cloudfront:ListCachePolicies",
      "cloudfront:ListResponseHeadersPolicies",
      "iam:GetOpenIDConnectProvider",
      "iam:ListOpenIDConnectProviders",
    ]
    resources = ["*"]
  }
}

resource "aws_iam_role_policy" "ci_plan" {
  name   = "plan"
  role   = aws_iam_role.ci_plan.id
  policy = data.aws_iam_policy_document.ci_plan.json
}

# For CI's variable AWZY_CORP_AWS_PLAN_ROLE_ARN.
output "ci_plan_role_arn" {
  value = aws_iam_role.ci_plan.arn
}

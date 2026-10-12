# The role CI's drift check assumes (.github/workflows/terraform-drift.yml): it plans the corp
# modules against their recorded state, without refreshing, to say when merged Terraform hasn't
# been applied. CI never applies Terraform; a person does. Read-only, and only what a plan
# without a refresh reads: the state, and the few things the modules look up. Only workflows on
# main can assume it.

resource "aws_iam_role" "ci_plan" {
  name                 = "awzy-ci-plan"
  assume_role_policy   = data.aws_iam_policy_document.ci_deploy_trust.json
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

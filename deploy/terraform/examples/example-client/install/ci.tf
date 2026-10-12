# The roles CI plans and applies the install account's modules as (ADR 0115,
# .github/workflows/terraform.yml): install, monitoring, grafana, and bootstrap. The
# example client grants them here, in its own account, as a client letting a vendor's pipeline
# manage an account would. Applied by hand once: a pipeline can't create its own role.

resource "aws_iam_openid_connect_provider" "github" {
  url            = "https://token.actions.githubusercontent.com"
  client_id_list = ["sts.amazonaws.com"]
}

data "aws_iam_policy_document" "ci_trust" {
  for_each = {
    # Pull requests and main plan, without refreshing, to list what merging changes.
    plan = ["repo:${var.github_oidc_subject_repo}:ref:refs/heads/main", "repo:${var.github_oidc_subject_repo}:pull_request"]
    # Only the example-aws environment, which waits for a person's approval, applies.
    terraform = ["repo:${var.github_oidc_subject_repo}:environment:example-aws"]
  }
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

resource "aws_iam_role" "ci" {
  for_each             = data.aws_iam_policy_document.ci_trust
  name                 = "awzy-example-ci-${each.key}"
  assume_role_policy   = each.value.json
  max_session_duration = 3600
}

resource "aws_iam_role_policy_attachment" "ci" {
  for_each = {
    plan      = "arn:aws:iam::aws:policy/ReadOnlyAccess"
    terraform = "arn:aws:iam::aws:policy/AdministratorAccess"
  }
  role       = aws_iam_role.ci[each.key].name
  policy_arn = each.value
}

# For CI's environment variables AWZY_CORP_EXAMPLE_AWS_PLAN_ROLE_ARN and _TERRAFORM_ROLE_ARN.
output "ci_role_arns" {
  value = { for k, r in aws_iam_role.ci : k => r.arn }
}

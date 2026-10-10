# The role CI assumes after publishing to deploy the corp portal and the registry
# (tools/ci/deploy-corp.sh). Like the publish role (corp/build), only pushes to main can
# assume it.

data "aws_iam_openid_connect_provider" "github" {
  url = "https://token.actions.githubusercontent.com"
}

data "aws_iam_policy_document" "ci_deploy_trust" {
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
    # GitHub's immutable subject for the repository (see corp/build).
    condition {
      test     = "StringEquals"
      variable = "token.actions.githubusercontent.com:sub"
      values   = ["repo:${var.github_oidc_subject_repo}:ref:refs/heads/main"]
    }
  }
}

resource "aws_iam_role" "ci_deploy" {
  name                 = "awzy-ci-deploy"
  assume_role_policy   = data.aws_iam_policy_document.ci_deploy_trust.json
  max_session_duration = 3600
}

data "aws_iam_policy_document" "ci_deploy" {
  # Task definitions have no resource-level permissions for describe and register.
  statement {
    actions   = ["ecs:DescribeTaskDefinition", "ecs:RegisterTaskDefinition"]
    resources = ["*"]
  }
  statement {
    actions = ["ecs:DescribeServices", "ecs:UpdateService"]
    resources = [
      "arn:aws:ecs:${var.region}:${var.build_account_id}:service/${aws_ecs_cluster.corp.name}/awzy-corp-portal",
      "arn:aws:ecs:${var.region}:${var.build_account_id}:service/${aws_ecs_cluster.corp.name}/awzy-registry",
    ]
  }
  # The new revisions keep the roles they had.
  statement {
    actions   = ["iam:PassRole"]
    resources = [aws_iam_role.execution.arn, aws_iam_role.registry_task.arn]
  }
}

resource "aws_iam_role_policy" "ci_deploy" {
  role   = aws_iam_role.ci_deploy.id
  policy = data.aws_iam_policy_document.ci_deploy.json
}

output "ci_deploy_role_arn" {
  value = aws_iam_role.ci_deploy.arn
}

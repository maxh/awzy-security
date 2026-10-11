# The role CI assumes after publishing to deploy the corp portal and the registry
# (tools/ci/deploy-corp.sh), and to promote a chart to the stable channel (tools/ci/promote.sh).
# Only workflows on main can assume it.

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
  # The migrate task (ADR 0087): the corp portal's new revision, run once before the services roll.
  statement {
    actions   = ["ecs:RunTask"]
    resources = ["arn:aws:ecs:${var.region}:${var.build_account_id}:task-definition/${local.name}:*"]
    condition {
      test     = "ArnEquals"
      variable = "ecs:cluster"
      values   = [aws_ecs_cluster.corp.arn]
    }
  }
  statement {
    actions   = ["ecs:DescribeTasks"]
    resources = ["arn:aws:ecs:${var.region}:${var.build_account_id}:task/${aws_ecs_cluster.corp.name}/*"]
  }
  # How long a chart has soaked in fast, before it's promoted to stable (tools/ci/promote.sh).
  statement {
    actions   = ["ecr:DescribeImages"]
    resources = ["arn:aws:ecr:${var.region}:${var.build_account_id}:repository/awzy/charts/awzy"]
  }
  # Which revision to deploy from (deploy-templates.tf).
  statement {
    actions   = ["ssm:GetParameter"]
    resources = ["arn:aws:ssm:${var.region}:${var.build_account_id}:parameter/awzy-corp-portal/task-definition-template/*"]
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

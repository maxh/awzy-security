# The role CI applies the build account's corp modules as, after merge (ADR 0115,
# .github/workflows/terraform.yml). It can change anything in this account, CI's other roles
# included, so only the GitHub environment corp-terraform can assume it, and that environment
# waits for a person's approval. Applied by hand once: a pipeline can't create its own role.

data "aws_iam_policy_document" "ci_terraform_trust" {
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
      values   = ["repo:${var.github_oidc_subject_repo}:environment:corp-terraform"]
    }
  }
}

resource "aws_iam_role" "ci_terraform" {
  name                 = "awzy-ci-terraform"
  assume_role_policy   = data.aws_iam_policy_document.ci_terraform_trust.json
  max_session_duration = 3600
}

resource "aws_iam_role_policy_attachment" "ci_terraform" {
  role       = aws_iam_role.ci_terraform.name
  policy_arn = "arn:aws:iam::aws:policy/AdministratorAccess"
}

# For CI's variable AWZY_CORP_AWS_TERRAFORM_ROLE_ARN.
output "ci_terraform_role_arn" {
  value = aws_iam_role.ci_terraform.arn
}

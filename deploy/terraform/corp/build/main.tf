# The build account: where releases are built and published. ECR repositories for
# every service image and the Helm chart, the cosign signing key, and the role GitHub
# Actions assumes to push (docs/deployment-plan.md, step 2).

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
    key          = "corp/build/terraform.tfstate"
    use_lockfile = true
  }
}

provider "aws" {
  region              = var.region
  profile             = var.build_profile
  allowed_account_ids = [var.build_account_id]
}

locals {
  # GitHub's immutable OIDC subject (owner and repository IDs), so a renamed or re-created
  # repository can't take over the role.
  github_subject_repo = var.github_oidc_subject_repo

  # One repository per published image (awzy_service binary_name).
  images = [
    "awzy-pdp", "awzy-gateway", "awzy-policy", "awzy-context", "awzy-connectors",
    "awzy-intake", "awzy-reasoner", "awzy-challenge", "awzy-clickhouse-migrate", "awzy-reporter",
    "awzy-client-portal", "awzy-corp-portal", "awzy-registry",
    # Only for the demo install (ADR 0032, 0039).
    "awzy-dev-idp", "awzy-synthetic",
  ]

  # Third-party images mirrored and signed for installs (ADR 0057), by name.
  third_party = [
    for l in split("\n", file("${path.module}/../../../release/third-party-images.txt")) : split(":", trimspace(l))[0]
    if trimspace(l) != "" && !startswith(trimspace(l), "#")
  ]
}

resource "aws_ecr_repository" "image" {
  for_each = toset(concat(
    [for i in local.images : "awzy/${i}"],
    [for i in local.third_party : "awzy/third-party/${i}"],
    ["awzy/charts/awzy"],
  ))

  name                 = each.key
  image_tag_mutability = "IMMUTABLE" # tags are commit SHAs; a tag never moves

  image_scanning_configuration {
    scan_on_push = true
  }
}

resource "aws_ecr_lifecycle_policy" "image" {
  for_each   = aws_ecr_repository.image
  repository = each.value.name
  policy = jsonencode({
    rules = [{
      rulePriority = 1
      description  = "Drop untagged manifests after 14 days"
      selection = {
        tagStatus   = "untagged"
        countType   = "sinceImagePushed"
        countUnit   = "days"
        countNumber = 14
      }
      action = { type = "expire" }
    }]
  })
}

# cosign signs image digests with this key; installs verify with its public key (Kyverno).
resource "aws_kms_key" "cosign" {
  description              = "cosign signing key for awzy release images"
  customer_master_key_spec = "ECC_NIST_P256"
  key_usage                = "SIGN_VERIFY"

  lifecycle {
    prevent_destroy = true
  }
}

resource "aws_kms_alias" "cosign" {
  name          = "alias/awzy-cosign"
  target_key_id = aws_kms_key.cosign.key_id
}

# GitHub Actions OIDC: CI gets short-lived credentials, no stored keys.
resource "aws_iam_openid_connect_provider" "github" {
  url            = "https://token.actions.githubusercontent.com"
  client_id_list = ["sts.amazonaws.com"]
}

data "aws_iam_policy_document" "ci_trust" {
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
    # Only pushes to main publish. Pull requests can't assume this role.
    condition {
      test     = "StringEquals"
      variable = "token.actions.githubusercontent.com:sub"
      values   = ["repo:${local.github_subject_repo}:ref:refs/heads/main"]
    }
  }
}

resource "aws_iam_role" "ci_publish" {
  name                 = "awzy-ci-publish"
  assume_role_policy   = data.aws_iam_policy_document.ci_trust.json
  max_session_duration = 3600
}

data "aws_iam_policy_document" "ci_publish" {
  statement {
    actions   = ["ecr:GetAuthorizationToken"]
    resources = ["*"]
  }
  statement {
    actions = [
      "ecr:BatchCheckLayerAvailability", "ecr:BatchGetImage", "ecr:GetDownloadUrlForLayer",
      "ecr:InitiateLayerUpload", "ecr:UploadLayerPart", "ecr:CompleteLayerUpload",
      "ecr:PutImage", "ecr:DescribeImages",
    ]
    resources = [for r in aws_ecr_repository.image : r.arn]
  }
  # Lets the publish script name a service whose repository is missing, before pushing anything.
  statement {
    actions   = ["ecr:DescribeRepositories"]
    resources = ["arn:aws:ecr:${var.region}:${var.build_account_id}:repository/awzy/*"]
  }
  statement {
    actions   = ["kms:Sign", "kms:GetPublicKey", "kms:DescribeKey"]
    resources = [aws_kms_key.cosign.arn]
  }
}

resource "aws_iam_role_policy" "ci_publish" {
  role   = aws_iam_role.ci_publish.id
  policy = data.aws_iam_policy_document.ci_publish.json
}

output "registry" {
  value = split("/", aws_ecr_repository.image["awzy/awzy-pdp"].repository_url)[0]
}

output "ci_role_arn" {
  value = aws_iam_role.ci_publish.arn
}

output "cosign_key" {
  value = "awskms:///${aws_kms_alias.cosign.arn}"
}

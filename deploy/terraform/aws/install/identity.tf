# EKS Pod Identity: one IAM role per workload, bound to a namespace and service account.
# The chart creates the service accounts; these names are part of install_contract.

data "aws_iam_policy_document" "pod_identity_trust" {
  statement {
    actions = ["sts:AssumeRole", "sts:TagSession"]
    principals {
      type        = "Service"
      identifiers = ["pods.eks.amazonaws.com"]
    }
  }
}

locals {
  workloads = {
    clickhouse = { namespace = "awzy-data", service_account = "clickhouse" }
    services   = { namespace = "awzy-system", service_account = "awzy" }
    portal     = { namespace = "awzy-portal", service_account = "awzy" }
    # Apart from the other services, so only awzy-policy may sign policy (signing.tf).
    policy = { namespace = "awzy-system", service_account = "awzy-policy" }
  }
}

resource "aws_iam_role" "workload" {
  for_each           = local.workloads
  name               = "${var.name}-${each.key}"
  assume_role_policy = data.aws_iam_policy_document.pod_identity_trust.json
}

resource "aws_eks_pod_identity_association" "workload" {
  for_each        = local.workloads
  cluster_name    = module.eks.cluster_name
  namespace       = each.value.namespace
  service_account = each.value.service_account
  role_arn        = aws_iam_role.workload[each.key].arn
}

# ClickHouse: cold tier and backups in the data bucket.
data "aws_iam_policy_document" "clickhouse" {
  statement {
    actions   = ["s3:ListBucket"]
    resources = [aws_s3_bucket.data.arn]
  }
  statement {
    actions   = ["s3:GetObject", "s3:PutObject", "s3:DeleteObject"]
    resources = ["${aws_s3_bucket.data.arn}/clickhouse/*"]
  }
  statement {
    actions   = ["kms:Encrypt", "kms:Decrypt", "kms:GenerateDataKey"]
    resources = [aws_kms_key.install.arn]
  }
}

resource "aws_iam_role_policy" "clickhouse" {
  role   = aws_iam_role.workload["clickhouse"].id
  policy = data.aws_iam_policy_document.clickhouse.json
}

# awzy services: envelope encryption, connector raw data, and the LLM.
data "aws_iam_policy_document" "services" {
  statement {
    actions   = ["kms:Encrypt", "kms:Decrypt", "kms:GenerateDataKey"]
    resources = [aws_kms_key.install.arn]
  }
  statement {
    actions   = ["s3:GetObject", "s3:PutObject"]
    resources = ["${aws_s3_bucket.data.arn}/connectors/*"]
  }
  statement {
    actions   = ["bedrock:InvokeModel", "bedrock:InvokeModelWithResponseStream"]
    resources = ["*"]
  }
  statement {
    actions   = ["secretsmanager:GetSecretValue"]
    resources = [aws_rds_cluster.postgres.master_user_secret[0].secret_arn]
  }
}

# The client portal: Awzy Bot calls Claude on Bedrock (ADR 0048), and nothing else.
data "aws_iam_policy_document" "portal" {
  statement {
    actions = ["bedrock:InvokeModel", "bedrock:InvokeModelWithResponseStream"]
    resources = [
      "arn:aws:bedrock:*::foundation-model/anthropic.*",
      "arn:aws:bedrock:*:${data.aws_caller_identity.current.account_id}:inference-profile/*anthropic.*",
    ]
  }
}

resource "aws_iam_role_policy" "portal" {
  role   = aws_iam_role.workload["portal"].id
  policy = data.aws_iam_policy_document.portal.json
}

resource "aws_iam_role_policy" "services" {
  role   = aws_iam_role.workload["services"].id
  policy = data.aws_iam_policy_document.services.json
}

# External Secrets Operator (installed by bootstrap) reads the Postgres credentials that RDS
# keeps in Secrets Manager, and the connectors' credentials.
resource "aws_iam_role" "external_secrets" {
  name               = "${var.name}-external-secrets"
  assume_role_policy = data.aws_iam_policy_document.pod_identity_trust.json
}

resource "aws_iam_role_policy" "external_secrets" {
  role = aws_iam_role.external_secrets.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = ["secretsmanager:GetSecretValue", "secretsmanager:DescribeSecret"]
        # The Postgres credentials, and the connectors' (ADR 0104).
        Resource = concat(
          [aws_rds_cluster.postgres.master_user_secret[0].secret_arn],
          [for s in aws_secretsmanager_secret.connector : s.arn],
        )
      },
      {
        # The connectors' secrets are encrypted with the install's key.
        Effect    = "Allow"
        Action    = ["kms:Decrypt"]
        Resource  = [aws_kms_key.install.arn]
        Condition = { StringEquals = { "kms:ViaService" = "secretsmanager.${data.aws_region.current.region}.amazonaws.com" } }
      },
    ]
  })
}

resource "aws_eks_pod_identity_association" "external_secrets" {
  cluster_name    = module.eks.cluster_name
  namespace       = "external-secrets"
  service_account = "external-secrets"
  role_arn        = aws_iam_role.external_secrets.arn
}

# Amazon Managed Grafana, signed in to through the account's IAM Identity Center. It reads AMP,
# CloudWatch (Aurora, MSK, ElastiCache), and X-Ray traces with its own role. deploy/terraform/grafana loads the data
# sources and dashboards, with the service account token below.
data "aws_iam_policy_document" "grafana_trust" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["grafana.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "grafana" {
  name               = "${var.name}-grafana"
  assume_role_policy = data.aws_iam_policy_document.grafana_trust.json
}

resource "aws_iam_role_policy_attachment" "grafana" {
  for_each = toset([
    "arn:aws:iam::aws:policy/AmazonPrometheusQueryAccess",
    "arn:aws:iam::aws:policy/service-role/AmazonGrafanaCloudWatchAccess",
    "arn:aws:iam::aws:policy/AWSXrayReadOnlyAccess",
  ])
  role       = aws_iam_role.grafana.name
  policy_arn = each.value
}

resource "aws_grafana_workspace" "awzy" {
  name                     = var.name
  account_access_type      = "CURRENT_ACCOUNT"
  authentication_providers = ["AWS_SSO"]
  permission_type          = "CUSTOMER_MANAGED"
  role_arn                 = aws_iam_role.grafana.arn
  data_sources             = ["PROMETHEUS", "CLOUDWATCH", "XRAY"]
}

resource "aws_grafana_role_association" "admins" {
  count        = length(var.grafana_admin_user_ids) > 0 ? 1 : 0
  workspace_id = aws_grafana_workspace.awzy.id
  role         = "ADMIN"
  user_ids     = var.grafana_admin_user_ids
}

# For deploy/terraform/grafana. Thirty days: apply this module again for a fresh one.
resource "aws_grafana_workspace_service_account" "terraform" {
  workspace_id = aws_grafana_workspace.awzy.id
  name         = "terraform"
  grafana_role = "ADMIN"
}

resource "aws_grafana_workspace_service_account_token" "terraform" {
  workspace_id       = aws_grafana_workspace.awzy.id
  service_account_id = aws_grafana_workspace_service_account.terraform.service_account_id
  name               = "terraform"
  seconds_to_live    = 30 * 24 * 3600
}

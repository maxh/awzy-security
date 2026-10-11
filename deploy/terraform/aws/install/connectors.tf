# The connectors' credentials (ADR 0104): one secret each, empty until a client admin sets it in
# the client portal. The portal may write them and never read them; External Secrets reads them
# into the cluster for the connectors.
resource "aws_secretsmanager_secret" "connector" {
  for_each   = toset(["drive", "jira"])
  name       = "${var.name}/connectors/${each.key}"
  kms_key_id = aws_kms_key.install.arn
  # Deleted at once with the install, so a new install of the same name can make it again.
  recovery_window_in_days = 0
}

# A first value, "unset", so External Secrets can read each secret before an admin sets it: a
# secret with no value at all fails its ExternalSecret, and with it every sync of the release.
# The connectors read a credential only once their connector is on. The portal's writes replace
# it, and Terraform never writes it again.
resource "aws_secretsmanager_secret_version" "connector_initial" {
  for_each      = aws_secretsmanager_secret.connector
  secret_id     = each.value.id
  secret_string = "unset"
  lifecycle {
    ignore_changes = [secret_string, version_stages]
  }
}

data "aws_iam_policy_document" "portal_connector_secrets" {
  statement {
    actions   = ["secretsmanager:PutSecretValue"]
    resources = [for s in aws_secretsmanager_secret.connector : s.arn]
  }
  # Secrets Manager encrypts the new value with the install's key on the writer's behalf.
  statement {
    actions   = ["kms:GenerateDataKey"]
    resources = [aws_kms_key.install.arn]
    condition {
      test     = "StringEquals"
      variable = "kms:ViaService"
      values   = ["secretsmanager.${data.aws_region.current.region}.amazonaws.com"]
    }
  }
}

resource "aws_iam_role_policy" "portal_connector_secrets" {
  role   = aws_iam_role.workload["portal"].id
  policy = data.aws_iam_policy_document.portal_connector_secrets.json
}

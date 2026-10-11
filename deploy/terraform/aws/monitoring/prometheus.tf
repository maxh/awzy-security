resource "aws_prometheus_workspace" "awzy" {
  alias = var.name
}

# The repository's alert rules, as every install gets them (deploy/observability, ADR 0068).
resource "aws_prometheus_rule_group_namespace" "awzy" {
  name         = "awzy"
  workspace_id = aws_prometheus_workspace.awzy.id
  data         = file("${path.module}/../../../observability/prometheus/alerts.yaml")
}

# Alerts go to an SNS topic, and from it to whoever subscribed.
resource "aws_sns_topic" "alerts" {
  name = "${var.name}-alerts"
}

data "aws_iam_policy_document" "alerts" {
  statement {
    actions   = ["sns:Publish", "sns:GetTopicAttributes"]
    resources = [aws_sns_topic.alerts.arn]
    principals {
      type        = "Service"
      identifiers = ["aps.amazonaws.com"]
    }
    condition {
      test     = "ArnEquals"
      variable = "aws:SourceArn"
      values   = [aws_prometheus_workspace.awzy.arn]
    }
  }
}

resource "aws_sns_topic_policy" "alerts" {
  arn    = aws_sns_topic.alerts.arn
  policy = data.aws_iam_policy_document.alerts.json
}

resource "aws_sns_topic_subscription" "alerts" {
  for_each  = toset(var.alert_emails)
  topic_arn = aws_sns_topic.alerts.arn
  protocol  = "email"
  endpoint  = each.value
}

resource "aws_prometheus_alert_manager_definition" "awzy" {
  workspace_id = aws_prometheus_workspace.awzy.id
  definition   = <<-YAML
    alertmanager_config: |
      route:
        receiver: sns
        group_by: [alertname, job]
        repeat_interval: 4h
      receivers:
        - name: sns
          sns_configs:
            - topic_arn: ${aws_sns_topic.alerts.arn}
              sigv4:
                region: ${data.aws_region.current.region}
              subject: 'awzy: {{ .CommonLabels.alertname }}'
  YAML
}

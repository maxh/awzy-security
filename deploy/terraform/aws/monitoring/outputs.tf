output "grafana" {
  description = "For deploy/terraform/grafana: where the workspace is, a token for it, and how its data sources authenticate"
  value = {
    url            = "https://${aws_grafana_workspace.awzy.endpoint}"
    token          = aws_grafana_workspace_service_account_token.terraform.key
    prometheus_url = aws_prometheus_workspace.awzy.prometheus_endpoint
    region         = data.aws_region.current.region
  }
  sensitive = true
}

output "alerts_topic_arn" {
  description = "The SNS topic alerts go to; subscribe anything else to it"
  value       = aws_sns_topic.alerts.arn
}

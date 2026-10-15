# The repository's alert rules, as every install gets them (deploy/observability, ADR 0068), each
# a Cloud Monitoring alert policy over Managed Prometheus's data with the same PromQL and `for`.
locals {
  rules = merge([
    for g in yamldecode(file("${path.module}/../../../observability/prometheus/alerts.yaml")).groups : {
      for r in g.rules : r.alert => merge(r, { group = g.name }) if can(r.alert)
    }
  ]...)
  # `for` in Prometheus's units, as seconds.
  unit_seconds = { s = 1, m = 60, h = 3600 }
  for_seconds = {
    for name, r in local.rules : name => (
      can(r["for"]) ? tonumber(regex("^([0-9]+)[smh]$", r["for"])[0]) * local.unit_seconds[regex("[smh]$", r["for"])] : 0
    )
  }
}

resource "google_monitoring_notification_channel" "email" {
  for_each     = toset(var.alert_emails)
  display_name = "${var.name} alerts: ${each.value}"
  type         = "email"
  labels       = { email_address = each.value }
}

resource "google_monitoring_alert_policy" "rule" {
  for_each     = local.rules
  display_name = "${var.name}: ${each.key}"
  combiner     = "OR"
  severity     = try(each.value.labels.severity, "") == "page" ? "CRITICAL" : "WARNING"

  conditions {
    display_name = each.key
    condition_prometheus_query_language {
      query               = each.value.expr
      duration            = "${local.for_seconds[each.key]}s"
      evaluation_interval = "60s"
      rule_group          = each.value.group
      alert_rule          = each.key
      labels              = try(each.value.labels, {})
    }
  }

  documentation {
    subject   = "awzy: ${each.key}"
    content   = "What to do: ${try(each.value.annotations.runbook_url, "the monitoring guide")}"
    mime_type = "text/markdown"
  }

  notification_channels = [for c in google_monitoring_notification_channel.email : c.id]
}

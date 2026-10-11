# awzy's dashboards in any Grafana (ADR 0096): a Prometheus data source with the uid the
# dashboards name, an optional CloudWatch one, and every dashboard in deploy/observability. The
# caller configures the grafana provider; aws/monitoring's grafana output has what an AWS install's
# Managed Grafana needs.

terraform {
  required_version = ">= 1.10"
  required_providers {
    grafana = {
      source  = "grafana/grafana"
      version = ">= 3.0"
    }
  }
}

variable "prometheus_url" {
  description = "The Prometheus (or AMP) query URL"
  type        = string
}

variable "prometheus_json_data" {
  description = "Extra jsonData for the Prometheus data source, e.g. SigV4 for AMP"
  type        = map(any)
  default     = {}
}

variable "cloudwatch_json_data" {
  description = "jsonData for a CloudWatch data source (Aurora, MSK, ElastiCache); null for none"
  type        = map(any)
  default     = null
}

# The dashboards' datasource uid (deploy/observability/README.md).
resource "grafana_data_source" "prometheus" {
  type              = "prometheus"
  name              = "awzy"
  uid               = "prometheus"
  url               = var.prometheus_url
  is_default        = true
  json_data_encoded = jsonencode(merge({ httpMethod = "POST" }, var.prometheus_json_data))
}

resource "grafana_data_source" "cloudwatch" {
  count             = var.cloudwatch_json_data == null ? 0 : 1
  type              = "cloudwatch"
  name              = "CloudWatch"
  uid               = "cloudwatch"
  json_data_encoded = jsonencode(var.cloudwatch_json_data)
}

resource "grafana_folder" "awzy" {
  title = "awzy"
}

resource "grafana_dashboard" "awzy" {
  for_each    = fileset("${path.module}/../../observability/grafana/dashboards", "*.json")
  folder      = grafana_folder.awzy.uid
  config_json = file("${path.module}/../../observability/grafana/dashboards/${each.value}")
  overwrite   = true
}

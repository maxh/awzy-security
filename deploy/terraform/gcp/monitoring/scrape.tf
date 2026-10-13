# One PodMonitoring per service, named after it: Managed Prometheus sets `job` to the
# PodMonitoring's name and won't let it be relabelled, and the rules and dashboards group by job.
locals {
  scraped = merge([for ns, names in var.services : { for n in names : n => ns }]...)
}

resource "kubernetes_manifest" "service" {
  for_each = local.scraped
  manifest = {
    apiVersion = "monitoring.googleapis.com/v1"
    kind       = "PodMonitoring"
    metadata   = { name = each.key, namespace = each.value }
    spec = {
      selector = { matchLabels = { "app.kubernetes.io/name" = each.key } }
      # The port that serves /metrics alone, which the chart lets any scraper reach (ADR 0124).
      endpoints    = [{ port = "metrics", interval = "30s" }]
      targetLabels = { metadata = ["pod", "container"] }
    }
  }
}

# Argo CD's controller: whether the install's release synced, and its health. Bootstrap opens its
# metrics port to scrapers.
resource "kubernetes_manifest" "argocd" {
  manifest = {
    apiVersion = "monitoring.googleapis.com/v1"
    kind       = "PodMonitoring"
    metadata   = { name = "argocd", namespace = var.ops_namespace }
    spec = {
      selector = { matchLabels = {
        "app.kubernetes.io/instance" = "argo-cd"
        "app.kubernetes.io/name"     = "argocd-application-controller"
      } }
      endpoints = [{ port = "metrics", interval = "30s" }]
    }
  }
}

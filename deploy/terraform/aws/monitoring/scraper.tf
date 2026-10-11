# AMP's managed scraper: agentless, so nothing more runs in the cluster. It finds pods through the
# Kubernetes API, with the read-only role below, and scrapes them from the cluster's own subnets.
resource "aws_prometheus_scraper" "awzy" {
  alias                = var.name
  scrape_configuration = templatefile("${path.module}/scrape.yaml.tftpl", { namespaces = var.namespaces })

  source {
    eks {
      cluster_arn        = data.aws_eks_cluster.this.arn
      subnet_ids         = var.network.subnet_ids
      security_group_ids = var.network.security_group_ids
    }
  }

  destination {
    amp {
      workspace_arn = aws_prometheus_workspace.awzy.arn
    }
  }
}

# Creating the scraper gives its service-linked role an EKS access entry of its own; making
# another fails ("invalid principal"). The role below is bound to the user name AMP's
# documentation gives the scraper.
locals {
  scraper_user = "aps-collector-user"
}

resource "kubernetes_cluster_role_v1" "scraper" {
  metadata {
    name = "aps-collector-role"
  }
  rule {
    api_groups = [""]
    resources  = ["nodes", "nodes/proxy", "nodes/metrics", "services", "endpoints", "pods"]
    verbs      = ["get", "list", "watch"]
  }
  rule {
    non_resource_urls = ["/metrics"]
    verbs             = ["get"]
  }
}

resource "kubernetes_cluster_role_binding_v1" "scraper" {
  metadata {
    name = "aps-collector-user-role-binding"
  }
  role_ref {
    api_group = "rbac.authorization.k8s.io"
    kind      = "ClusterRole"
    name      = kubernetes_cluster_role_v1.scraper.metadata[0].name
  }
  subject {
    kind      = "User"
    name      = local.scraper_user
    api_group = "rbac.authorization.k8s.io"
  }
}

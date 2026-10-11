# AMP's managed scraper: agentless, so nothing more runs in the cluster. It finds pods through the
# Kubernetes API, with the access entry and read-only role below, and scrapes them from the
# cluster's own subnets.
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

locals {
  # The scraper's service-linked role, without its path, as an EKS access entry takes it.
  scraper_role = replace(aws_prometheus_scraper.awzy.role_arn, "/role\\/aws-service-role\\/[^/]+\\//", "role/")
  scraper_user = "aps-collector-user"
}

resource "aws_eks_access_entry" "scraper" {
  cluster_name  = var.network.cluster_name
  principal_arn = local.scraper_role
  user_name     = local.scraper_user
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

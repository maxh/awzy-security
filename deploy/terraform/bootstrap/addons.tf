resource "helm_release" "argo_cd" {
  name       = "argo-cd"
  namespace  = kubernetes_namespace_v1.awzy["ops"].metadata[0].name
  repository = "https://argoproj.github.io/argo-helm"
  chart      = "argo-cd"
  version    = var.versions.argo_cd
  values = [yamlencode({
    # Pull only: no UI or API exposed outside the cluster, no Dex.
    dex     = { enabled = false }
    configs = { params = { "server.insecure" = true } }
  })]
}

resource "helm_release" "kyverno" {
  name             = "kyverno"
  namespace        = "kyverno"
  create_namespace = true
  repository       = "https://kyverno.github.io/kyverno/"
  chart            = "kyverno"
  version          = var.versions.kyverno
}

resource "helm_release" "external_secrets" {
  name             = "external-secrets"
  namespace        = "external-secrets"
  create_namespace = true
  repository       = "https://charts.external-secrets.io"
  chart            = "external-secrets"
  version          = var.versions.external_secrets
  # The per-cloud module binds workload identity to this service account name.
  values = [yamlencode({ serviceAccount = { name = "external-secrets" } })]
}

resource "helm_release" "clickhouse_operator" {
  name       = "clickhouse-operator"
  namespace  = kubernetes_namespace_v1.awzy["ops"].metadata[0].name
  repository = "https://docs.altinity.com/clickhouse-operator/"
  chart      = "altinity-clickhouse-operator"
  version    = var.versions.clickhouse_operator
}

# Ingress: Gateway API with Envoy Gateway (same on every cloud; the chart also installs the
# Gateway API CRDs). cert-manager gets certificates from Let's Encrypt for Gateway listeners.
resource "helm_release" "envoy_gateway" {
  name             = "envoy-gateway"
  namespace        = "envoy-gateway-system"
  create_namespace = true
  chart            = "oci://docker.io/envoyproxy/gateway-helm"
  version          = var.versions.envoy_gateway
}

resource "helm_release" "cert_manager" {
  name             = "cert-manager"
  namespace        = "cert-manager"
  create_namespace = true
  repository       = "https://charts.jetstack.io"
  chart            = "cert-manager"
  version          = var.versions.cert_manager
  values = [yamlencode({
    crds = { enabled = true }
    config = {
      apiVersion       = "controller.config.cert-manager.io/v1alpha1"
      kind             = "ControllerConfiguration"
      enableGatewayAPI = true
    }
  })]

  # cert-manager looks for the Gateway API CRDs when it starts.
  depends_on = [helm_release.envoy_gateway]
}

# Resources that need the CRDs above: the secret store, the signature policy, ingress, and
# the Argo CD Application for the awzy chart.
resource "helm_release" "config" {
  name      = "awzy-config"
  namespace = kubernetes_namespace_v1.awzy["ops"].metadata[0].name
  chart     = "${path.module}/charts/awzy-config"

  values = [yamlencode({
    namespaces  = local.namespaces
    secretStore = local.c.secret_store
    registry    = { prefix = local.c.registry_prefix }
    ingress = {
      hostname           = var.ingress.hostname
      acmeEmail          = var.ingress.acme_email
      serviceAnnotations = local.c.load_balancer.service_annotations
    }
    cosignPubKey = var.cosign_public_key
    chart        = { repository = "${local.registry_host}/awzy/charts", version = var.chart_version }
    awzyValues = {
      image = { registry = local.c.registry_prefix }
      postgres = {
        host      = local.c.postgres.host
        port      = local.c.postgres.port
        database  = local.c.postgres.database
        secretRef = local.c.postgres.secret_arn
      }
      kafka      = { brokers = local.c.kafka.bootstrap_brokers }
      clickhouse = { storageClass = local.c.storage_class.name }
      demoIdp    = { enabled = var.demo_idp }
      demoData   = { enabled = var.demo_data }
      reporter   = var.reporter == null ? { enabled = false, installId = "" } : { enabled = true, installId = nonsensitive(var.reporter.install_id) }
      clientPortal = {
        auth      = { issuer = var.client_portal.issuer, jwksUrl = var.client_portal.jwks_url }
        publicUrl = coalesce(var.client_portal.public_url, "https://${var.ingress.hostname}")
        hostname  = var.ingress.hostname
        awzyBot   = { enabled = var.awzy_bot && local.c.cloud == "aws", region = local.c.region }
      }
      gateway = { name = "awzy", namespace = local.namespaces.ops }
    }
  })]

  depends_on = [
    helm_release.argo_cd, helm_release.kyverno, helm_release.external_secrets,
    helm_release.clickhouse_operator, helm_release.envoy_gateway, helm_release.cert_manager,
    kubernetes_secret_v1.clickhouse, kubernetes_secret_v1.reporting_token, kubernetes_secret_v1.scim_token,
    kubernetes_secret_v1.registry, kubernetes_secret_v1.registry_kyverno, kubernetes_secret_v1.chart_repo,
  ]
}

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
    # Mirrored images only in awzy's namespaces (ADR 0057).
    global = {
      image            = local.mirrored["argocd"]
      imagePullSecrets = [{ name = "awzy-registry" }]
    }
    redis = { image = local.mirrored["redis"] }
  })]
  depends_on = [kubernetes_secret_v1.registry]
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
  values = [yamlencode({
    imagePullSecrets = [{ name = "awzy-registry" }]
    # The operator runs in ops but ClickHouse is in data; by default it watches only its own namespace.
    configs  = { files = { "config.yaml" = { watch = { namespaces = { include = [local.namespaces.data] } } } } }
    operator = { image = local.mirrored["clickhouse-operator"] }
    metrics  = { image = local.mirrored["metrics-exporter"] }
    # The CRD hook needs a shell and kubectl; alpine/k8s has both (the chart's default,
    # bitnami/kubectl:latest, is unpinned and no longer maintained).
    crdHook = {
      image            = local.mirrored["k8s"]
      imagePullSecrets = [{ name = "awzy-registry" }]
    }
  })]
  depends_on = [kubernetes_secret_v1.registry]
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
    # HTTP-01 solver pods run next to the Gateway, in awzy-ops, so they use the mirror too.
    acmesolver = { image = local.mirrored["cert-manager-acmesolver"] }
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
      extraHostnames = {
        mcp    = try(var.mcp_gateway.hostname == null ? "" : var.mcp_gateway.hostname, "")
        intake = try(var.intake.hostname == null ? "" : var.intake.hostname, "")
      }
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
      kafka = { brokers = local.c.kafka.bootstrap_brokers, tls = local.c.kafka.tls }
      policy = {
        breakGlassKeysSecret = var.break_glass_public_keys == null ? "" : "awzy-policy-break-glass"
        signingKmsKey        = local.c.policy_signing.kms_key
      }
      clickhouse = { storageClass = local.c.storage_class.name }
      demoIdp    = { enabled = var.demo_idp }
      demoData   = { enabled = var.demo_data, activity = { enabled = var.demo_activity } }
      reporter   = var.reporter == null ? { enabled = false, installId = "" } : { enabled = true, installId = nonsensitive(var.reporter.install_id) }
      clientPortal = {
        supportPublicKey = var.support_public_key == null ? "" : var.support_public_key
        auth = {
          issuer       = var.client_portal.issuer
          jwksUrl      = var.client_portal.jwks_url
          audience     = var.client_portal.audience
          mcpAudience  = var.client_portal.mcp_audience
          subjectClaim = var.client_portal.subject_claim
        }
        publicUrl = coalesce(var.client_portal.public_url, "https://${var.ingress.hostname}")
        hostname  = var.ingress.hostname
        awzyBot   = { enabled = var.awzy_bot && local.c.cloud == "aws", region = local.c.region, model = var.llm_model }
      }
      reasoner = {
        investigations = {
          enabled          = var.investigations && local.c.cloud == "aws"
          region           = local.c.region
          model            = var.llm_model
          monthlyTokens    = var.investigation_budget.monthly_tokens
          privilegedGroups = var.investigation_budget.privileged_groups
        }
      }
      challenge = {
        slack = var.slack == null ? { tokenSecret = "", securityChannel = "" } : {
          tokenSecret     = "awzy-slack-token"
          securityChannel = nonsensitive(var.slack.security_channel)
        }
      }
      gateway = { name = "awzy", namespace = local.namespaces.ops }
      mcpGateway = {
        enabled   = var.mcp_gateway != null
        hostname  = try(var.mcp_gateway.hostname == null ? "" : var.mcp_gateway.hostname, "")
        auth      = { audience = try(var.mcp_gateway.audience, "awzy-gateway") }
        upstreams = try(var.mcp_gateway.upstreams, [])
        outage    = { cachedAllowTtlSecs = 300 }
      }
      intake = {
        enabled  = var.intake != null
        hostname = try(var.intake.hostname == null ? "" : var.intake.hostname, "")
      }
      connectors = {
        drive = {
          subject      = var.drive == null ? "" : nonsensitive(var.drive.subject)
          adminSubject = var.drive == null ? "" : nonsensitive(var.drive.admin_subject)
          domain       = var.drive == null ? "" : nonsensitive(var.drive.domain)
        }
        jira = {
          url      = var.jira == null ? "" : nonsensitive(var.jira.url)
          user     = var.jira == null ? "" : nonsensitive(var.jira.user)
          projects = var.jira == null ? [] : nonsensitive(var.jira.projects)
        }
      }
    }
  })]

  depends_on = [
    helm_release.argo_cd, helm_release.kyverno, helm_release.external_secrets,
    helm_release.clickhouse_operator, helm_release.envoy_gateway, helm_release.cert_manager,
    kubernetes_secret_v1.clickhouse, kubernetes_secret_v1.reporting_token, kubernetes_secret_v1.scim_token,
    kubernetes_secret_v1.intake_token, kubernetes_secret_v1.drive, kubernetes_secret_v1.jira,
    kubernetes_secret_v1.registry, kubernetes_secret_v1.registry_kyverno, kubernetes_secret_v1.chart_repo,
    kubernetes_secret_v1.policy_verify, kubernetes_secret_v1.break_glass,
  ]
}

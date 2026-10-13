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

# Argo CD's chart lets only the cluster's pods reach its controller's metrics. A managed scraper
# (AMP's on AWS) comes from outside any pod, so this opens the metrics port, and only it: the
# release's sync and health, which the install's alerts watch (deploy/observability).
resource "kubernetes_network_policy_v1" "argo_cd_metrics" {
  metadata {
    name      = "argo-cd-controller-metrics"
    namespace = kubernetes_namespace_v1.awzy["ops"].metadata[0].name
  }
  spec {
    pod_selector {
      match_labels = {
        "app.kubernetes.io/instance" = "argo-cd"
        "app.kubernetes.io/name"     = "argocd-application-controller"
      }
    }
    policy_types = ["Ingress"]
    ingress {
      ports {
        port     = "metrics"
        protocol = "TCP"
      }
    }
  }
}

resource "helm_release" "kyverno" {
  name             = "kyverno"
  namespace        = "kyverno"
  create_namespace = true
  repository       = "https://kyverno.github.io/kyverno/"
  chart            = "kyverno"
  version          = var.versions.kyverno
  # Verifying an image loads its SBOM attestation into the policy's context, and a third-party
  # image's SBOM can pass Kyverno's 2 MiB default (ClickHouse's is 3.6 MB), which refuses the pod.
  values = [yamlencode({ config = { maxContextSize = "16Mi" } })]
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

# The resource metrics API, for the chart's HorizontalPodAutoscalers. Most managed clusters don't
# install it; one that already does sets metrics_server = false.
resource "helm_release" "metrics_server" {
  count      = var.metrics_server ? 1 : 0
  name       = "metrics-server"
  namespace  = "kube-system"
  repository = "https://kubernetes-sigs.github.io/metrics-server/"
  chart      = "metrics-server"
  version    = var.versions.metrics_server
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
      redirectHostnames  = var.ingress.redirect_hostnames
      serviceAnnotations = local.c.load_balancer.service_annotations
      extraHostnames = {
        mcp    = try(var.mcp_gateway.hostname == null ? "" : var.mcp_gateway.hostname, "")
        intake = try(var.intake.hostname == null ? "" : var.intake.hostname, "")
      }
    }
    cosignPubKey = var.cosign_public_key
    chart        = { repository = "${local.registry_host}/awzy/charts", version = var.chart_version }
    awzyValues = {
      bootstrap = { level = local.bootstrap_level }
      image     = { registry = local.c.registry_prefix }
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
      # Which IdP groups are teams (ADR 0117); empty leaves the chart's default.
      context = { teamGroups = join(",", var.team_groups) }
      clickhouse = {
        storageClass  = local.c.storage_class.name
        retentionDays = var.clickhouse_retention_days
        backup        = { url = local.c.object_store.url == "" ? "" : "${local.c.object_store.url}/clickhouse/backups" }
      }
      tracing = var.tracing == null ? { otlpEndpoint = "", sampleRatio = 1 } : {
        otlpEndpoint = var.tracing.otlp_endpoint
        sampleRatio  = var.tracing.sample_ratio
      }
      demoIdp  = { enabled = var.demo_idp }
      demoData = { enabled = var.demo_data, activity = { enabled = var.demo_activity } }
      reporter = var.reporter == null ? { enabled = false, installId = "" } : { enabled = true, installId = nonsensitive(var.reporter.install_id) }
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
        awzyBot   = { enabled = var.awzy_bot && local.llm_routing_json != "" }
      }
      # What the models can reach, and the routing to start with (ADR 0120).
      models = {
        connections = local.llm_connections
        routing     = local.llm_routing_json == "" ? null : jsondecode(local.llm_routing_json)
        apiKeySecrets = merge(
          { for env in nonsensitive(keys(var.llm_api_keys)) : env => { secret = "awzy-llm-keys", key = env } },
          var.llm_api_key_secrets,
        )
      }
      reasoner = {
        investigations = {
          enabled          = var.investigations && local.llm_routing_json != ""
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
      # Settings come from the client portal; credentials from the cloud's secrets manager
      # (ADR 0104).
      connectors = {
        drive = { secretRef = lookup(local.c.connector_secrets, "drive", "") }
        jira  = { secretRef = lookup(local.c.connector_secrets, "jira", "") }
      }
    }
  })]

  depends_on = [
    helm_release.argo_cd, helm_release.kyverno, helm_release.external_secrets,
    helm_release.clickhouse_operator, helm_release.envoy_gateway, helm_release.cert_manager,
    kubernetes_secret_v1.clickhouse, kubernetes_secret_v1.reporting_token, kubernetes_secret_v1.scim_token,
    kubernetes_secret_v1.intake_token,
    kubernetes_secret_v1.registry, kubernetes_secret_v1.registry_kyverno, kubernetes_secret_v1.chart_repo,
    kubernetes_secret_v1.policy_verify, kubernetes_secret_v1.break_glass,
  ]
}

# Cloud-agnostic layer of an awzy install (ADR 0015). Takes install_contract from a per-cloud
# module (aws/install, later gcp/install, azure/install) and never branches on the cloud.
# Installs Argo CD, Kyverno (signature checks), External Secrets, and the ClickHouse operator,
# then hands the awzy chart to Argo CD.
#
# The caller configures the helm and kubernetes providers for the target cluster.

terraform {
  required_version = ">= 1.10"
  required_providers {
    helm = {
      source  = "hashicorp/helm"
      version = "~> 3.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.38"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.6"
    }
  }
}

locals {
  c = var.install_contract

  # What this bootstrap (and the install contract it reads) provides, as one number the chart checks
  # before it installs (ADR 0098). Raise it when a chart starts needing something new from here or
  # from the cloud layer (a Secret, a key, a value), and raise the chart's bootstrap.required to
  # match in the same change; tools/ci/check-bootstrap-level.sh keeps the two equal.
  bootstrap_level = 1

  namespaces = {
    system = "awzy-system"
    data   = "awzy-data"
    portal = "awzy-portal"
    ops    = "awzy-ops"
  }

  # The registry host, e.g. registry.awzysecurity.com
  registry_host = split("/", local.c.registry_prefix)[0]

  # Third-party images, mirrored into the release registry and signed (ADR 0057): name =>
  # { repository, tag }, from the list CI mirrors. Changing a component's version in
  # var.versions means mirroring its new images first. The tag carries the digest
  # (`<tag>@sha256:…`): Kyverno rejects any later change to a Deployment whose image has none.
  mirror_lines = [
    for l in split("\n", file("${path.module}/../../release/third-party-images.txt")) :
    regex("^([a-z0-9-]+):(\\S+)\\s+\\S+@(sha256:[0-9a-f]+)$", trimspace(l))
    if trimspace(l) != "" && !startswith(trimspace(l), "#")
  ]
  mirrored = { for m in local.mirror_lines : m[0] => { repository = "${local.c.registry_prefix}/third-party/${m[0]}", tag = "${m[1]}@${m[2]}" } }
}

resource "kubernetes_namespace_v1" "awzy" {
  for_each = local.namespaces
  metadata {
    name   = each.value
    labels = { "app.kubernetes.io/part-of" = "awzy" }
  }
}

resource "kubernetes_storage_class_v1" "default" {
  metadata {
    name        = local.c.storage_class.name
    annotations = { "storageclass.kubernetes.io/is-default-class" = "true" }
  }
  storage_provisioner    = local.c.storage_class.provisioner
  parameters             = local.c.storage_class.parameters
  volume_binding_mode    = "WaitForFirstConsumer"
  allow_volume_expansion = true
}

# ClickHouse runs in the cluster, so its password is made here rather than in a cloud
# secret store.
resource "random_password" "clickhouse" {
  length  = 32
  special = false
}

resource "kubernetes_secret_v1" "clickhouse" {
  for_each = toset(["data", "portal"]) # ClickHouse itself, and the portal that queries it
  metadata {
    name      = "awzy-clickhouse"
    namespace = kubernetes_namespace_v1.awzy[each.key].metadata[0].name
  }
  data = { password = random_password.clickhouse.result }
}

# awzy-reporter's read-only ClickHouse user (ADR 0042): the migrate Job in the data namespace
# sets it, and the reporter in the system namespace signs in with it.
resource "random_password" "clickhouse_reporter" {
  length  = 32
  special = false
}

resource "kubernetes_secret_v1" "clickhouse_reporter" {
  for_each = toset(["data", "system"])
  metadata {
    name      = "awzy-clickhouse-reporter"
    namespace = kubernetes_namespace_v1.awzy[each.key].metadata[0].name
  }
  data = { password = random_password.clickhouse_reporter.result }
}

# awzy-reasoner's read-only ClickHouse user (ADR 0056), set by the migrate Job like the
# reporter's.
resource "random_password" "clickhouse_reasoner" {
  length  = 32
  special = false
}

resource "kubernetes_secret_v1" "clickhouse_reasoner" {
  for_each = toset(["data", "system"])
  metadata {
    name      = "awzy-clickhouse-reasoner"
    namespace = kubernetes_namespace_v1.awzy[each.key].metadata[0].name
  }
  data = { password = random_password.clickhouse_reasoner.result }
}

# awzy-challenge's Slack bot token, for notifications (ADR 0073).
resource "kubernetes_secret_v1" "slack_token" {
  count = var.slack == null ? 0 : 1
  metadata {
    name      = "awzy-slack-token"
    namespace = kubernetes_namespace_v1.awzy["system"].metadata[0].name
  }
  data = { token = var.slack.token }
}

# Sources send events to awzy-intake with this token. Read it from the `intake` output.
resource "random_password" "intake" {
  count   = var.intake == null ? 0 : 1
  length  = 48
  special = false
}

# In the portal namespace too: the client portal records its MCP calls as activity (ADR 0084).
resource "kubernetes_secret_v1" "intake_token" {
  for_each = var.intake == null ? toset([]) : toset(["system", "portal"])
  metadata {
    name      = "awzy-intake-token"
    namespace = kubernetes_namespace_v1.awzy[each.key].metadata[0].name
  }
  data = { token = random_password.intake[0].result }
}

output "intake" {
  description = "Where sources send events, and the token they send"
  value = var.intake == null ? null : {
    url   = try(var.intake.hostname, null) == null ? "http://awzy-intake.${local.namespaces.system}.svc:8080/v1/events" : "https://${var.intake.hostname}/v1/events"
    token = random_password.intake[0].result
  }
  sensitive = true
}

# The connectors' credentials (ADR 0069, 0070).
resource "kubernetes_secret_v1" "drive" {
  count = var.drive == null ? 0 : 1
  metadata {
    name      = "awzy-connectors-drive"
    namespace = kubernetes_namespace_v1.awzy["system"].metadata[0].name
  }
  data = { "key.json" = var.drive.key_json }
}

resource "kubernetes_secret_v1" "jira" {
  count = var.jira == null ? 0 : 1
  metadata {
    name      = "awzy-connectors-jira"
    namespace = kubernetes_namespace_v1.awzy["system"].metadata[0].name
  }
  data = { token = var.jira.token }
}

# The reporter sends heartbeats to the corp portal with this token (ADR 0026).
resource "kubernetes_secret_v1" "reporting_token" {
  count = var.reporter == null ? 0 : 1
  metadata {
    name      = "awzy-reporting-token"
    namespace = kubernetes_namespace_v1.awzy["system"].metadata[0].name
  }
  data = { token = var.reporter.token }
}

# The bearer token the client's IdP uses for SCIM provisioning into awzy-context (ADR 0031).
# Read it once from the `scim` output and paste it into the IdP's provisioning settings.
resource "random_password" "scim" {
  length  = 48
  special = false
}

resource "kubernetes_secret_v1" "scim_token" {
  metadata {
    name      = "awzy-scim-token"
    namespace = kubernetes_namespace_v1.awzy["system"].metadata[0].name
  }
  data = { token = random_password.scim.result }
}

output "scim" {
  description = "SCIM endpoint and bearer token for the client's IdP (Okta, Entra)"
  value = {
    url   = "https://${var.ingress.hostname}/scim/v2"
    token = random_password.scim.result
  }
  sensitive = true
}

# awzy-policy signs every bundle, enforcement mode change, and outage fallback with the install's
# KMS key, and the PDPs and gateways apply only what verifies against its public key (ADR 0021,
# 0080, 0092), so writing to awzy-policy's database doesn't change what's enforced. The private
# key never leaves KMS; only the public key is here. To rotate, add the new key's public key to
# this secret, roll the PDPs and gateways, then point awzy-policy at the new key.
resource "kubernetes_secret_v1" "policy_verify" {
  metadata {
    name      = "awzy-policy-verify"
    namespace = kubernetes_namespace_v1.awzy["system"].metadata[0].name
  }
  data = { "verify.pem" = local.c.policy_signing.public_key_pem }
}

# The public keys of the admins enrolled for break-glass rollback (ADR 0022).
resource "kubernetes_secret_v1" "break_glass" {
  count = var.break_glass_public_keys == null ? 0 : 1
  metadata {
    name      = "awzy-policy-break-glass"
    namespace = kubernetes_namespace_v1.awzy["system"].metadata[0].name
  }
  data = { "keys.pem" = var.break_glass_public_keys }
}

# Pull credentials for registry.awzysecurity.com (ADR 0028): one static, revocable token per
# install. The kubelet uses it in the awzy namespaces (ops too, for the mirrored images of Argo
# CD, the ClickHouse operator, and cert-manager's solver), Kyverno to read signatures, and Argo
# CD to pull the chart. Nothing refreshes it.
locals {
  dockerconfig = jsonencode({
    auths = {
      (local.registry_host) = {
        username = var.registry_credentials.username
        password = var.registry_credentials.password
        auth     = base64encode("${var.registry_credentials.username}:${var.registry_credentials.password}")
      }
    }
  })
}

resource "kubernetes_secret_v1" "registry" {
  for_each = toset(["system", "data", "portal", "ops"])
  metadata {
    name      = "awzy-registry"
    namespace = kubernetes_namespace_v1.awzy[each.key].metadata[0].name
  }
  type = "kubernetes.io/dockerconfigjson"
  data = { ".dockerconfigjson" = local.dockerconfig }
}

resource "kubernetes_secret_v1" "registry_kyverno" {
  metadata {
    name      = "awzy-registry"
    namespace = helm_release.kyverno.namespace
  }
  type = "kubernetes.io/dockerconfigjson"
  data = { ".dockerconfigjson" = local.dockerconfig }
}

resource "kubernetes_secret_v1" "chart_repo" {
  metadata {
    name      = "awzy-chart-repo"
    namespace = kubernetes_namespace_v1.awzy["ops"].metadata[0].name
    labels    = { "argocd.argoproj.io/secret-type" = "repository" }
  }
  data = {
    type      = "helm"
    name      = "awzy"
    url       = "${local.registry_host}/awzy/charts"
    enableOCI = "true"
    username  = var.registry_credentials.username
    password  = var.registry_credentials.password
  }
}

# Kafka runs in the cluster, with Strimzi (the platform plan's in-cluster option), rather than
# Google Cloud's Managed Service for Apache Kafka: that one requires SASL, which awzy's Kafka client
# and ClickHouse's Kafka engine don't speak yet, and costs more than the rest of a small install.
# One KRaft node for the small profile; seven days' retention, as on AWS (ADR 0006).

locals {
  kafka_namespace = "kafka"
  kafka_nodes = {
    small    = { replicas = 1, storage = "20Gi", cpu = "500m", memory = "2Gi" }
    standard = { replicas = 3, storage = "100Gi", cpu = "1", memory = "4Gi" }
    large    = { replicas = 3, storage = "500Gi", cpu = "2", memory = "8Gi" }
  }[var.size_profile]
}

resource "helm_release" "strimzi" {
  name             = "strimzi"
  namespace        = local.kafka_namespace
  create_namespace = true
  repository       = "https://strimzi.io/charts/"
  chart            = "strimzi-kafka-operator"
  version          = "1.2.0"
  depends_on       = [google_container_node_pool.general]
}

# The Kafka cluster itself, from charts/kafka: Terraform can't plan custom resources whose CRDs
# the operator above hasn't installed yet, and Helm doesn't need to.
resource "helm_release" "kafka" {
  name      = "awzy-kafka"
  namespace = local.kafka_namespace
  chart     = "${path.module}/charts/kafka"
  values = [yamlencode({
    name         = "awzy"
    replicas     = local.kafka_nodes.replicas
    storage      = local.kafka_nodes.storage
    storageClass = "standard-rwo" # GKE's own pd-balanced class; bootstrap's comes later
    resources    = { requests = { cpu = local.kafka_nodes.cpu, memory = local.kafka_nodes.memory }, limits = { memory = local.kafka_nodes.memory } }
  })]
  depends_on = [helm_release.strimzi]
}

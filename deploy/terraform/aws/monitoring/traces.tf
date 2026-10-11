# Traces (ADR 0097): an OpenTelemetry collector in the cluster that awzy's services send spans to
# over OTLP, keeps every slow or failed trace and a sample of the rest, and writes them to AWS
# X-Ray, which the Grafana workspace reads. Spans stay in the install's account.

locals {
  traces_namespace = "awzy-observability"
  collector        = "otel-collector"
  # contrib carries the tail sampler and the X-Ray exporter. Pinned by digest; multi-arch.
  collector_image = "otel/opentelemetry-collector-contrib:0.161.0@sha256:fd328de2552466ad78385e1b1289c3f2402b1c45f265b252aab1955b42845ac1"

  collector_config = {
    receivers = {
      otlp = { protocols = { grpc = { endpoint = "0.0.0.0:4317" } } }
    }
    processors = {
      memory_limiter = { check_interval = "1s", limit_percentage = 80, spike_limit_percentage = 20 }
      # A trace is decided once its spans have arrived, 10 s after the first.
      tail_sampling = {
        decision_wait = "10s"
        policies = [
          { name = "errors", type = "status_code", status_code = { status_codes = ["ERROR"] } },
          { name = "slow", type = "latency", latency = { threshold_ms = var.traces.slow_ms } },
          { name = "sample", type = "probabilistic", probabilistic = { sampling_percentage = var.traces.sample_percent } },
        ]
      }
      batch = {}
    }
    exporters = {
      awsxray = { region = data.aws_region.current.region }
    }
    extensions = {
      health_check = { endpoint = "0.0.0.0:13133" }
    }
    service = {
      extensions = ["health_check"]
      pipelines = {
        traces = {
          receivers  = ["otlp"]
          processors = ["memory_limiter", "tail_sampling", "batch"]
          exporters  = ["awsxray"]
        }
      }
    }
  }
}

resource "kubernetes_namespace_v1" "traces" {
  metadata {
    name = local.traces_namespace
  }
}

resource "kubernetes_service_account_v1" "collector" {
  metadata {
    name      = local.collector
    namespace = kubernetes_namespace_v1.traces.metadata[0].name
  }
}

data "aws_iam_policy_document" "collector_trust" {
  statement {
    actions = ["sts:AssumeRole", "sts:TagSession"]
    principals {
      type        = "Service"
      identifiers = ["pods.eks.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "collector" {
  name               = "${var.name}-otel-collector"
  assume_role_policy = data.aws_iam_policy_document.collector_trust.json
}

resource "aws_iam_role_policy_attachment" "collector" {
  role       = aws_iam_role.collector.name
  policy_arn = "arn:aws:iam::aws:policy/AWSXrayWriteOnlyAccess"
}

resource "aws_eks_pod_identity_association" "collector" {
  cluster_name    = var.network.cluster_name
  namespace       = local.traces_namespace
  service_account = local.collector
  role_arn        = aws_iam_role.collector.arn
}

# EKS takes a few seconds to apply a new association, and a pod started sooner never gets its
# credentials: on the example install, the collector's first pod couldn't write to X-Ray.
resource "time_sleep" "collector_identity" {
  create_duration = "30s"
  depends_on      = [aws_eks_pod_identity_association.collector]
}

resource "kubernetes_config_map_v1" "collector" {
  metadata {
    name      = local.collector
    namespace = kubernetes_namespace_v1.traces.metadata[0].name
  }
  data = {
    "config.yaml" = yamlencode(local.collector_config)
  }
}

resource "kubernetes_deployment_v1" "collector" {
  metadata {
    name      = local.collector
    namespace = kubernetes_namespace_v1.traces.metadata[0].name
    labels    = { "app.kubernetes.io/name" = local.collector }
  }
  spec {
    # One replica: tail sampling needs every span of a trace in the same collector.
    replicas = 1
    selector {
      match_labels = { "app.kubernetes.io/name" = local.collector }
    }
    template {
      metadata {
        labels = { "app.kubernetes.io/name" = local.collector }
        # A config change rolls the pod.
        annotations = { "awzy.dev/config-hash" = sha256(yamlencode(local.collector_config)) }
      }
      spec {
        service_account_name = kubernetes_service_account_v1.collector.metadata[0].name
        security_context {
          run_as_non_root = true
          run_as_user     = 10001
          seccomp_profile {
            type = "RuntimeDefault"
          }
        }
        container {
          name  = "collector"
          image = local.collector_image
          args  = ["--config=/etc/otelcol/config.yaml"]
          port {
            name           = "otlp-grpc"
            container_port = 4317
          }
          readiness_probe {
            http_get {
              path = "/"
              port = 13133
            }
          }
          resources {
            requests = { cpu = "50m", memory = "128Mi" }
            limits   = { memory = "512Mi" }
          }
          security_context {
            allow_privilege_escalation = false
            read_only_root_filesystem  = true
            capabilities {
              drop = ["ALL"]
            }
          }
          volume_mount {
            name       = "config"
            mount_path = "/etc/otelcol"
            read_only  = true
          }
        }
        volume {
          name = "config"
          config_map {
            name = kubernetes_config_map_v1.collector.metadata[0].name
          }
        }
      }
    }
  }
  depends_on = [time_sleep.collector_identity]
}

resource "kubernetes_service_v1" "collector" {
  metadata {
    name      = local.collector
    namespace = kubernetes_namespace_v1.traces.metadata[0].name
  }
  spec {
    selector = { "app.kubernetes.io/name" = local.collector }
    port {
      name        = "otlp-grpc"
      port        = 4317
      target_port = "otlp-grpc"
    }
  }
}

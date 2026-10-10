# The one interface to deploy/terraform/bootstrap (ADR 0015). Every cloud's install module
# outputs this same shape.
output "install_contract" {
  value = {
    cloud        = "aws"
    region       = data.aws_region.current.region
    size_profile = var.size_profile
    cluster = {
      name     = module.eks.cluster_name
      endpoint = module.eks.cluster_endpoint
    }
    postgres = {
      host       = aws_rds_cluster.postgres.endpoint
      port       = aws_rds_cluster.postgres.port
      database   = aws_rds_cluster.postgres.database_name
      secret_arn = aws_rds_cluster.postgres.master_user_secret[0].secret_arn
    }
    kafka = {
      bootstrap_brokers     = aws_msk_cluster.kafka.bootstrap_brokers
      bootstrap_brokers_tls = aws_msk_cluster.kafka.bootstrap_brokers_tls
    }
    valkey = {
      host = aws_elasticache_serverless_cache.valkey.endpoint[0].address
      port = aws_elasticache_serverless_cache.valkey.endpoint[0].port
    }
    object_store = {
      bucket = aws_s3_bucket.data.id
    }
    kms_key_id = aws_kms_key.install.arn
    # Pod Identity binds roles to these service accounts; no annotations needed.
    workload_identity = {
      for k, w in local.workloads : "${w.namespace}/${w.service_account}" => aws_iam_role.workload[k].arn
    }
    # bootstrap creates both from this data, so it never needs to know the cloud.
    storage_class = {
      name        = "gp3"
      provisioner = "ebs.csi.aws.com"
      parameters  = { type = "gp3", encrypted = "true" }
    }
    # Annotations for the Service that fronts the Gateway (bootstrap passes them through):
    # an internet-facing NLB from the EKS in-tree load balancer controller.
    load_balancer = {
      service_annotations = {
        "service.beta.kubernetes.io/aws-load-balancer-type" = "nlb"
      }
    }
    # External Secrets provider block (ClusterSecretStore spec.provider); credentials come
    # from Pod Identity on the external-secrets service account.
    secret_store = {
      aws = {
        service = "SecretsManager"
        region  = data.aws_region.current.region
      }
    }
    registry_prefix = var.registry_prefix
  }
}

resource "aws_kms_key" "install" {
  description         = "${var.name}: envelope encryption for data at rest"
  enable_key_rotation = true
}

resource "aws_kms_alias" "install" {
  name          = "alias/${var.name}"
  target_key_id = aws_kms_key.install.key_id
}

# Raw connector data, the ClickHouse cold tier, and backups.
resource "aws_s3_bucket" "data" {
  bucket = "${var.name}-data-${data.aws_caller_identity.current.account_id}"
}

resource "aws_s3_bucket_server_side_encryption_configuration" "data" {
  bucket = aws_s3_bucket.data.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm     = "aws:kms"
      kms_master_key_id = aws_kms_key.install.arn
    }
    bucket_key_enabled = true
  }
}

resource "aws_s3_bucket_public_access_block" "data" {
  bucket                  = aws_s3_bucket.data.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

# --- Postgres: Aurora Serverless v2, pausing to zero when idle in the small profile.

resource "aws_db_subnet_group" "postgres" {
  name       = var.name
  subnet_ids = module.vpc.private_subnets
}

resource "aws_rds_cluster" "postgres" {
  cluster_identifier          = var.name
  engine                      = "aurora-postgresql"
  engine_version              = "17.9"
  database_name               = "awzy"
  master_username             = "awzy"
  manage_master_user_password = true # password lives in Secrets Manager
  db_subnet_group_name        = aws_db_subnet_group.postgres.name
  vpc_security_group_ids      = [aws_security_group.data.id]
  storage_encrypted           = true
  kms_key_id                  = aws_kms_key.install.arn
  backup_retention_period     = 7
  deletion_protection         = var.deletion_protection
  skip_final_snapshot         = !var.deletion_protection
  final_snapshot_identifier   = var.deletion_protection ? "${var.name}-final" : null

  serverlessv2_scaling_configuration {
    min_capacity             = var.size_profile == "small" ? 0 : 0.5
    max_capacity             = 4
    seconds_until_auto_pause = var.size_profile == "small" ? 600 : null
  }
}

resource "aws_rds_cluster_instance" "postgres" {
  identifier         = "${var.name}-1"
  cluster_identifier = aws_rds_cluster.postgres.id
  instance_class     = "db.serverless"
  engine             = aws_rds_cluster.postgres.engine
  engine_version     = aws_rds_cluster.postgres.engine_version
}

# --- Kafka: MSK provisioned. Decisions and events are kept 7 days so ClickHouse can replay
# (ADR 0006). awzy's services and ClickHouse connect over TLS (install_contract hands them the TLS
# brokers); brokers are reachable only from EKS nodes.

# MSK runs KRaft only on m5 and m7g brokers, so the small profile keeps t3.small brokers on 3.9,
# the last line with ZooKeeper: about a fifth of the cost, and awzy needs nothing KRaft adds.
locals {
  kafka_version = var.size_profile == "small" ? "3.9.x" : "4.1.x.kraft"
  kafka_broker  = var.size_profile == "small" ? "kafka.t3.small" : "kafka.m7g.large"
}

resource "aws_msk_configuration" "kafka" {
  name              = var.name
  kafka_versions    = [local.kafka_version]
  server_properties = <<-PROPERTIES
    auto.create.topics.enable=false
    default.replication.factor=2
    min.insync.replicas=1
    log.retention.hours=168
  PROPERTIES
}

resource "aws_msk_cluster" "kafka" {
  cluster_name           = var.name
  kafka_version          = local.kafka_version
  number_of_broker_nodes = length(var.azs)

  broker_node_group_info {
    instance_type   = local.kafka_broker
    client_subnets  = module.vpc.private_subnets
    security_groups = [aws_security_group.data.id]
    storage_info {
      ebs_storage_info {
        volume_size = 20
      }
    }
  }

  configuration_info {
    arn      = aws_msk_configuration.kafka.arn
    revision = aws_msk_configuration.kafka.latest_revision
  }

  encryption_info {
    encryption_at_rest_kms_key_arn = aws_kms_key.install.arn
    encryption_in_transit {
      # Plaintext stays open until installs have moved to TLS without a gap; then TLS only.
      client_broker = "TLS_PLAINTEXT"
      in_cluster    = true
    }
  }

  # Network-scoped for now: only EKS nodes reach the brokers. SASL/SCRAM comes with the
  # PDP's Kafka client settings.
  client_authentication {
    unauthenticated = true
  }
}

# --- Valkey: ElastiCache Serverless.

resource "aws_elasticache_serverless_cache" "valkey" {
  name                 = var.name
  engine               = "valkey"
  major_engine_version = "8"
  subnet_ids           = module.vpc.private_subnets
  security_group_ids   = [aws_security_group.data.id]
  kms_key_id           = aws_kms_key.install.arn

  cache_usage_limits {
    data_storage {
      maximum = 1
      unit    = "GB"
    }
    ecpu_per_second {
      maximum = 5000
    }
  }
}

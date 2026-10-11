resource "random_password" "postgres" {
  length  = 40
  special = false
}

resource "aws_rds_cluster" "postgres" {
  cluster_identifier        = local.name
  engine                    = "aurora-postgresql"
  engine_version            = "17.9"
  database_name             = "awzy_corp"
  master_username           = "awzy"
  master_password           = random_password.postgres.result
  db_subnet_group_name      = module.vpc.database_subnet_group_name
  vpc_security_group_ids    = [aws_security_group.db.id]
  storage_encrypted         = true
  backup_retention_period   = 7
  deletion_protection       = true
  final_snapshot_identifier = "${local.name}-final"

  # Pauses to zero when nobody is using the corp portal; the first request after a pause
  # waits about 15 seconds while it resumes.
  serverlessv2_scaling_configuration {
    min_capacity             = 0
    max_capacity             = 2
    seconds_until_auto_pause = 900
  }
}

resource "aws_rds_cluster_instance" "postgres" {
  identifier         = "${local.name}-1"
  cluster_identifier = aws_rds_cluster.postgres.id
  instance_class     = "db.serverless"
  engine             = aws_rds_cluster.postgres.engine
  engine_version     = aws_rds_cluster.postgres.engine_version
}

resource "aws_secretsmanager_secret" "postgres_url" {
  name = "${local.name}/postgres-url"
}

resource "aws_secretsmanager_secret_version" "postgres_url" {
  secret_id     = aws_secretsmanager_secret.postgres_url.id
  secret_string = "postgres://awzy:${random_password.postgres.result}@${aws_rds_cluster.postgres.endpoint}:5432/awzy_corp?sslmode=require"
}

resource "random_password" "session" {
  length  = 64
  special = false
}

resource "aws_secretsmanager_secret" "session" {
  name = "${local.name}/session-secret"
}

resource "aws_secretsmanager_secret_version" "session" {
  secret_id     = aws_secretsmanager_secret.session.id
  secret_string = random_password.session.result
}

# The staff accounts file (ADR 0025): a JSON list of argon2id hashes. Its value is set outside
# Terraform (see README), so hashes and passwords never pass through state.
resource "aws_secretsmanager_secret" "accounts" {
  name = "${local.name}/accounts"
}

# The Slack incoming webhook fleet alerts go to (ADR 0043). Terraform stores a placeholder, which
# turns alerts off, and never overwrites the real URL once someone sets it (see README):
#   aws secretsmanager put-secret-value --secret-id awzy-corp-portal/alerts-slack-webhook \
#     --secret-string 'https://hooks.slack.com/services/…'
resource "aws_secretsmanager_secret" "alerts_slack_webhook" {
  name = "${local.name}/alerts-slack-webhook"
}

resource "aws_secretsmanager_secret_version" "alerts_slack_webhook" {
  secret_id     = aws_secretsmanager_secret.alerts_slack_webhook.id
  secret_string = "unset"
  lifecycle {
    ignore_changes = [secret_string]
  }
}

# The key the corp portal signs support session tokens with (ADR 0095): a PKCS#8 PEM P-256 private
# key, whose public half every install's client portal trusts (deploy/release/support.pub). Set by
# hand, never in Terraform state (see README); the placeholder leaves support sessions unable to
# sign in.
resource "aws_secretsmanager_secret" "support_signing_key" {
  name = "${local.name}/support-signing-key"
}

resource "aws_secretsmanager_secret_version" "support_signing_key" {
  secret_id     = aws_secretsmanager_secret.support_signing_key.id
  secret_string = "unset"
  lifecycle {
    ignore_changes = [secret_string]
  }
}

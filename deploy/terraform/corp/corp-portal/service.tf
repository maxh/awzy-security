resource "aws_ecs_cluster" "corp" {
  name = local.name
}

resource "aws_cloudwatch_log_group" "corp" {
  name              = "/ecs/${local.name}"
  retention_in_days = 30
}

data "aws_iam_policy_document" "ecs_tasks_trust" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["ecs-tasks.amazonaws.com"]
    }
  }
}

# Pulls the image, writes logs, and reads the three secrets into the containers.
resource "aws_iam_role" "execution" {
  name               = "${local.name}-execution"
  assume_role_policy = data.aws_iam_policy_document.ecs_tasks_trust.json
}

resource "aws_iam_role_policy_attachment" "execution" {
  role       = aws_iam_role.execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

resource "aws_iam_role_policy" "execution_secrets" {
  role = aws_iam_role.execution.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Action = ["secretsmanager:GetSecretValue"]
      Resource = [
        aws_secretsmanager_secret.postgres_url.arn,
        aws_secretsmanager_secret.session.arn,
        aws_secretsmanager_secret.accounts.arn,
        aws_secretsmanager_secret.alerts_slack_webhook.arn,
      ]
    }]
  })
}

locals {
  log_config = {
    logDriver = "awslogs"
    options = {
      awslogs-group         = aws_cloudwatch_log_group.corp.name
      awslogs-region        = data.aws_region.current.region
      awslogs-stream-prefix = "corp"
    }
  }
}

resource "aws_ecs_task_definition" "corp" {
  family                   = local.name
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = 256
  memory                   = 512
  execution_role_arn       = aws_iam_role.execution.arn

  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "X86_64"
  }

  volume {
    name = "config"
  }

  container_definitions = jsonencode([
    {
      # The corp portal reads accounts from a file; this writes the secret to one.
      name             = "accounts"
      image            = "public.ecr.aws/docker/library/busybox:1.37"
      essential        = false
      command          = ["sh", "-c", "printf '%s' \"$ACCOUNTS\" > ${local.accounts_dir}/accounts.json"]
      secrets          = [{ name = "ACCOUNTS", valueFrom = aws_secretsmanager_secret.accounts.arn }]
      mountPoints      = [{ sourceVolume = "config", containerPath = local.accounts_dir }]
      logConfiguration = local.log_config
    },
    {
      name         = "corp-portal"
      image        = local.image
      essential    = true
      dependsOn    = [{ containerName = "accounts", condition = "SUCCESS" }]
      portMappings = [{ containerPort = 8080, protocol = "tcp" }]
      environment = [
        { name = "AWZY_CORP_PORTAL_ACCOUNTS_FILE", value = "${local.accounts_dir}/accounts.json" },
        { name = "AWZY_CORP_PORTAL_PUBLIC_URL", value = "https://${local.hostname}" },
        { name = "AWZY_LOG_FORMAT", value = "json" },
        { name = "AWZY_RELEASE_VERSION", value = var.image_tag },
      ]
      secrets = [
        { name = "AWZY_CORP_PORTAL_ALERTS_SLACK_WEBHOOK_URL", valueFrom = aws_secretsmanager_secret.alerts_slack_webhook.arn },
        { name = "AWZY_CORP_PORTAL_SESSION_SECRET", valueFrom = aws_secretsmanager_secret.session.arn },
        { name = "AWZY_POSTGRES_URL", valueFrom = aws_secretsmanager_secret.postgres_url.arn },
      ]
      mountPoints            = [{ sourceVolume = "config", containerPath = local.accounts_dir, readOnly = true }]
      readonlyRootFilesystem = true
      logConfiguration       = local.log_config
    },
  ])
}

resource "aws_ecs_service" "corp" {
  name            = local.name
  cluster         = aws_ecs_cluster.corp.id
  task_definition = aws_ecs_task_definition.corp.arn
  desired_count   = 1 # sign-in lockouts are per replica (ADR 0025)
  launch_type     = "FARGATE"

  network_configuration {
    subnets          = module.vpc.public_subnets
    security_groups  = [aws_security_group.app.id]
    assign_public_ip = true # reaches ECR and Secrets Manager without a NAT gateway
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.corp.arn
    container_name   = "corp-portal"
    container_port   = 8080
  }

  deployment_circuit_breaker {
    enable   = true
    rollback = true
  }

  # CI points the service at a new task definition revision for each commit on main;
  # Terraform doesn't move it back.
  lifecycle {
    ignore_changes = [task_definition]
  }

  depends_on = [aws_lb_listener.https]
}

output "url" {
  value = "https://${local.hostname}"
}

output "accounts_secret" {
  value = aws_secretsmanager_secret.accounts.name
}

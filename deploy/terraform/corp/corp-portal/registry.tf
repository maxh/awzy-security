# registry.<domain>: awzy-registry, the pull-only proxy in front of ECR (ADR 0028).
# Same load balancer (host rule), same Postgres (its own "registry" schema) as the corp portal.

locals {
  registry_enabled = var.registry_image_tag != ""
  registry_image   = "${local.ecr_host}/awzy/awzy-registry:${var.registry_image_tag}"
}

resource "aws_lb_target_group" "registry" {
  name        = "awzy-registry"
  port        = 8080
  protocol    = "HTTP"
  target_type = "ip"
  vpc_id      = module.vpc.vpc_id

  health_check {
    path    = "/readyz"
    matcher = "200"
  }
  deregistration_delay = 15
}

resource "aws_lb_listener_rule" "registry" {
  listener_arn = aws_lb_listener.https.arn
  priority     = 10

  condition {
    host_header {
      values = [local.registry_hostname]
    }
  }
  action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.registry.arn
  }
}

resource "cloudflare_dns_record" "registry" {
  zone_id = local.cf_zone_id
  name    = local.registry_hostname
  type    = "CNAME"
  content = aws_lb.corp.dns_name
  ttl     = 300
  proxied = false
  comment = "release registry proxy (deploy/terraform/corp/corp-portal)"
}

# The proxy reads ECR in this account: it gets a registry token and serves manifests; clients
# follow ECR's presigned layer URLs themselves.
resource "aws_iam_role" "registry_task" {
  name               = "awzy-registry-task"
  assume_role_policy = data.aws_iam_policy_document.ecs_tasks_trust.json
}

resource "aws_iam_role_policy" "registry_task" {
  role = aws_iam_role.registry_task.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      { Effect = "Allow", Action = ["ecr:GetAuthorizationToken"], Resource = "*" },
      {
        Effect   = "Allow"
        Action   = ["ecr:BatchGetImage", "ecr:GetDownloadUrlForLayer", "ecr:BatchCheckLayerAvailability", "ecr:DescribeImages", "ecr:ListImages"]
        Resource = "arn:aws:ecr:${var.region}:${var.build_account_id}:repository/awzy/*"
      },
    ]
  })
}

resource "aws_ecs_task_definition" "registry" {
  count                    = local.registry_enabled ? 1 : 0
  family                   = "awzy-registry"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = 256
  memory                   = 512
  execution_role_arn       = aws_iam_role.execution.arn
  task_role_arn            = aws_iam_role.registry_task.arn

  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "X86_64"
  }

  container_definitions = jsonencode([{
    name         = "registry"
    image        = local.registry_image
    essential    = true
    portMappings = [{ containerPort = 8080, protocol = "tcp" }]
    environment = [
      { name = "AWZY_REGISTRY_UPSTREAM", value = local.ecr_host },
      { name = "AWZY_REGISTRY_REALM", value = local.registry_hostname },
      { name = "AWZY_LOG_FORMAT", value = "json" },
    ]
    secrets                = [{ name = "AWZY_POSTGRES_URL", valueFrom = aws_secretsmanager_secret.postgres_url.arn }]
    readonlyRootFilesystem = true
    logConfiguration = merge(local.log_config, {
      options = merge(local.log_config.options, { awslogs-stream-prefix = "registry" })
    })
  }])
}

resource "aws_ecs_service" "registry" {
  count           = local.registry_enabled ? 1 : 0
  name            = "awzy-registry"
  cluster         = aws_ecs_cluster.corp.id
  task_definition = aws_ecs_task_definition.registry[0].arn
  desired_count   = 1 # two once real clients pull through it (ADR 0028)
  launch_type     = "FARGATE"

  network_configuration {
    subnets          = module.vpc.public_subnets
    security_groups  = [aws_security_group.app.id]
    assign_public_ip = true
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.registry.arn
    container_name   = "registry"
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

  depends_on = [aws_lb_listener_rule.registry]
}

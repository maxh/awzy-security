resource "aws_acm_certificate" "corp" {
  domain_name               = local.hostname
  subject_alternative_names = [local.registry_hostname]
  validation_method         = "DNS"

  lifecycle {
    create_before_destroy = true
  }
}

resource "cloudflare_dns_record" "cert_validation" {
  for_each = {
    for o in aws_acm_certificate.corp.domain_validation_options : o.domain_name => o
  }
  zone_id = local.cf_zone_id
  name    = trimsuffix(each.value.resource_record_name, ".")
  type    = each.value.resource_record_type
  content = trimsuffix(each.value.resource_record_value, ".")
  ttl     = 300
  proxied = false
  comment = "ACM validation for ${local.hostname} (deploy/terraform/corp/corp-portal)"
}

resource "aws_acm_certificate_validation" "corp" {
  certificate_arn         = aws_acm_certificate.corp.arn
  validation_record_fqdns = [for r in cloudflare_dns_record.cert_validation : r.name]
}

resource "aws_lb" "corp" {
  name                       = local.name
  load_balancer_type         = "application"
  subnets                    = module.vpc.public_subnets
  security_groups            = [aws_security_group.alb.id]
  drop_invalid_header_fields = true
}

resource "aws_lb_target_group" "corp" {
  name        = local.name
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

resource "aws_lb_listener" "https" {
  load_balancer_arn = aws_lb.corp.arn
  port              = 443
  protocol          = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = aws_acm_certificate_validation.corp.certificate_arn

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.corp.arn
  }
}

resource "aws_lb_listener" "http" {
  load_balancer_arn = aws_lb.corp.arn
  port              = 80
  protocol          = "HTTP"

  default_action {
    type = "redirect"
    redirect {
      protocol    = "HTTPS"
      port        = "443"
      status_code = "HTTP_301"
    }
  }
}

resource "cloudflare_dns_record" "corp" {
  zone_id = local.cf_zone_id
  name    = local.hostname
  type    = "CNAME"
  content = aws_lb.corp.dns_name
  ttl     = 300
  proxied = false # TLS ends at the load balancer with the ACM certificate
  comment = "corp portal (deploy/terraform/corp/corp-portal)"
}

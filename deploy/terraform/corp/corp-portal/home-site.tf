# https://<domain> and https://www.<domain>: one page that says what awzy is and links to the source,
# the docs, and the demo (home/index.html.tftpl). Built like the docs site (docs-site.tf): a
# private S3 bucket that only CloudFront reads, its own certificate, DNS on Cloudflare, DNS
# only. Terraform uploads the page itself, so there's no CI step.

variable "repository_url" {
  description = "The public source repository the home page links to, e.g. https://github.com/owner/repo"
  type        = string
}

locals {
  home_hostnames = [var.domain, "www.${var.domain}"]
}

resource "aws_s3_bucket" "home" {
  bucket = "awzy-home-${var.build_account_id}"
}

resource "aws_s3_bucket_public_access_block" "home" {
  bucket                  = aws_s3_bucket.home.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_ownership_controls" "home" {
  bucket = aws_s3_bucket.home.id
  rule {
    object_ownership = "BucketOwnerEnforced"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "home" {
  bucket = aws_s3_bucket.home.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_object" "home_index" {
  bucket        = aws_s3_bucket.home.id
  key           = "index.html"
  content       = templatefile("${path.module}/home/index.html.tftpl", { domain = var.domain, repository_url = var.repository_url })
  content_type  = "text/html; charset=utf-8"
  cache_control = "public, max-age=300"
}

# Only this distribution reads the bucket.
data "aws_iam_policy_document" "home_bucket" {
  statement {
    actions   = ["s3:GetObject"]
    resources = ["${aws_s3_bucket.home.arn}/*"]
    principals {
      type        = "Service"
      identifiers = ["cloudfront.amazonaws.com"]
    }
    condition {
      test     = "StringEquals"
      variable = "AWS:SourceArn"
      values   = [aws_cloudfront_distribution.home.arn]
    }
  }
}

resource "aws_s3_bucket_policy" "home" {
  bucket = aws_s3_bucket.home.id
  policy = data.aws_iam_policy_document.home_bucket.json
}

resource "aws_acm_certificate" "home" {
  provider                  = aws.us_east_1
  domain_name               = var.domain
  subject_alternative_names = ["www.${var.domain}"]
  validation_method         = "DNS"

  lifecycle {
    create_before_destroy = true
  }
}

resource "cloudflare_dns_record" "home_cert_validation" {
  for_each = {
    for o in aws_acm_certificate.home.domain_validation_options : o.domain_name => o
  }
  zone_id = local.cf_zone_id
  name    = trimsuffix(each.value.resource_record_name, ".")
  type    = each.value.resource_record_type
  content = trimsuffix(each.value.resource_record_value, ".")
  ttl     = 300
  proxied = false
  comment = "ACM validation for ${each.key} (deploy/terraform/corp/corp-portal/home-site.tf)"
}

resource "aws_acm_certificate_validation" "home" {
  provider                = aws.us_east_1
  certificate_arn         = aws_acm_certificate.home.arn
  validation_record_fqdns = [for r in cloudflare_dns_record.home_cert_validation : r.name]
}

resource "aws_cloudfront_origin_access_control" "home" {
  name                              = "awzy-home"
  origin_access_control_origin_type = "s3"
  signing_behavior                  = "always"
  signing_protocol                  = "sigv4"
}

resource "aws_cloudfront_distribution" "home" {
  enabled             = true
  aliases             = local.home_hostnames
  default_root_object = "index.html"
  http_version        = "http2and3"
  price_class         = "PriceClass_100"
  comment             = "awzy home page (deploy/terraform/corp/corp-portal/home-site.tf)"

  origin {
    origin_id                = "home"
    domain_name              = aws_s3_bucket.home.bucket_regional_domain_name
    origin_access_control_id = aws_cloudfront_origin_access_control.home.id
  }

  default_cache_behavior {
    target_origin_id           = "home"
    viewer_protocol_policy     = "redirect-to-https"
    allowed_methods            = ["GET", "HEAD"]
    cached_methods             = ["GET", "HEAD"]
    compress                   = true
    cache_policy_id            = data.aws_cloudfront_cache_policy.optimized.id
    response_headers_policy_id = data.aws_cloudfront_response_headers_policy.security.id
  }

  # Any other path is a 403 from S3 (CloudFront may not list the bucket): show the page.
  custom_error_response {
    error_code         = 403
    response_code      = 404
    response_page_path = "/index.html"
  }

  restrictions {
    geo_restriction {
      restriction_type = "none"
    }
  }

  viewer_certificate {
    acm_certificate_arn      = aws_acm_certificate_validation.home.certificate_arn
    ssl_support_method       = "sni-only"
    minimum_protocol_version = "TLSv1.2_2021"
  }
}

# Cloudflare flattens a CNAME at the zone's apex, so it can sit beside the mail records there.
resource "cloudflare_dns_record" "home" {
  for_each = toset(local.home_hostnames)
  zone_id  = local.cf_zone_id
  name     = each.value
  type     = "CNAME"
  content  = aws_cloudfront_distribution.home.domain_name
  ttl      = 300
  proxied  = false # TLS ends at CloudFront with the ACM certificate
  comment  = "home page (deploy/terraform/corp/corp-portal/home-site.tf)"
}

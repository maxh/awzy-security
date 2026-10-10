# https://docs.<domain>: awzy's public documentation (docs/site). A private S3 bucket that only
# CloudFront can read, behind CloudFront with its own certificate; DNS stays on Cloudflare, DNS
# only, like the corp portal. CI uploads each build of main with the deploy role below
# (.github/workflows/docs-site.yml), so no long-lived credential leaves AWS.

# CloudFront takes certificates from us-east-1 only.
provider "aws" {
  alias               = "us_east_1"
  region              = "us-east-1"
  profile             = var.build_profile
  allowed_account_ids = [var.build_account_id]
}

locals {
  docs_hostname = "docs.${var.domain}"
}

resource "aws_s3_bucket" "docs" {
  bucket = "awzy-docs-${var.build_account_id}"
}

resource "aws_s3_bucket_public_access_block" "docs" {
  bucket                  = aws_s3_bucket.docs.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_ownership_controls" "docs" {
  bucket = aws_s3_bucket.docs.id
  rule {
    object_ownership = "BucketOwnerEnforced"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "docs" {
  bucket = aws_s3_bucket.docs.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

# Only this distribution reads the bucket.
data "aws_iam_policy_document" "docs_bucket" {
  statement {
    actions   = ["s3:GetObject"]
    resources = ["${aws_s3_bucket.docs.arn}/*"]
    principals {
      type        = "Service"
      identifiers = ["cloudfront.amazonaws.com"]
    }
    condition {
      test     = "StringEquals"
      variable = "AWS:SourceArn"
      values   = [aws_cloudfront_distribution.docs.arn]
    }
  }
}

resource "aws_s3_bucket_policy" "docs" {
  bucket = aws_s3_bucket.docs.id
  policy = data.aws_iam_policy_document.docs_bucket.json
}

resource "aws_acm_certificate" "docs" {
  provider          = aws.us_east_1
  domain_name       = local.docs_hostname
  validation_method = "DNS"

  lifecycle {
    create_before_destroy = true
  }
}

resource "cloudflare_dns_record" "docs_cert_validation" {
  for_each = {
    for o in aws_acm_certificate.docs.domain_validation_options : o.domain_name => o
  }
  zone_id = local.cf_zone_id
  name    = trimsuffix(each.value.resource_record_name, ".")
  type    = each.value.resource_record_type
  content = trimsuffix(each.value.resource_record_value, ".")
  ttl     = 300
  proxied = false
  comment = "ACM validation for ${local.docs_hostname} (deploy/terraform/corp/corp-portal/docs-site.tf)"
}

resource "aws_acm_certificate_validation" "docs" {
  provider                = aws.us_east_1
  certificate_arn         = aws_acm_certificate.docs.arn
  validation_record_fqdns = [for r in cloudflare_dns_record.docs_cert_validation : r.name]
}

resource "aws_cloudfront_origin_access_control" "docs" {
  name                              = "awzy-docs"
  origin_access_control_origin_type = "s3"
  signing_behavior                  = "always"
  signing_protocol                  = "sigv4"
}

# Astro builds /concepts/risk/ as /concepts/risk/index.html.
resource "aws_cloudfront_function" "docs_index" {
  name    = "awzy-docs-index"
  runtime = "cloudfront-js-2.0"
  publish = true
  code    = <<-JS
    function handler(event) {
      var request = event.request;
      var uri = request.uri;
      if (uri.endsWith("/")) {
        request.uri = uri + "index.html";
      } else if (!uri.split("/").pop().includes(".")) {
        request.uri = uri + "/index.html";
      }
      return request;
    }
  JS
}

data "aws_cloudfront_cache_policy" "optimized" {
  name = "Managed-CachingOptimized"
}

data "aws_cloudfront_response_headers_policy" "security" {
  name = "Managed-SecurityHeadersPolicy"
}

resource "aws_cloudfront_distribution" "docs" {
  enabled             = true
  aliases             = [local.docs_hostname]
  default_root_object = "index.html"
  http_version        = "http2and3"
  price_class         = "PriceClass_100"
  comment             = "awzy docs (deploy/terraform/corp/corp-portal/docs-site.tf)"

  origin {
    origin_id                = "docs"
    domain_name              = aws_s3_bucket.docs.bucket_regional_domain_name
    origin_access_control_id = aws_cloudfront_origin_access_control.docs.id
  }

  default_cache_behavior {
    target_origin_id           = "docs"
    viewer_protocol_policy     = "redirect-to-https"
    allowed_methods            = ["GET", "HEAD"]
    cached_methods             = ["GET", "HEAD"]
    compress                   = true
    cache_policy_id            = data.aws_cloudfront_cache_policy.optimized.id
    response_headers_policy_id = data.aws_cloudfront_response_headers_policy.security.id
    function_association {
      event_type   = "viewer-request"
      function_arn = aws_cloudfront_function.docs_index.arn
    }
  }

  # A missing page is a 403 from S3 (CloudFront may not list the bucket): show Starlight's 404.
  custom_error_response {
    error_code         = 403
    response_code      = 404
    response_page_path = "/404.html"
  }

  restrictions {
    geo_restriction {
      restriction_type = "none"
    }
  }

  viewer_certificate {
    acm_certificate_arn      = aws_acm_certificate_validation.docs.certificate_arn
    ssl_support_method       = "sni-only"
    minimum_protocol_version = "TLSv1.2_2021"
  }
}

resource "cloudflare_dns_record" "docs" {
  zone_id = local.cf_zone_id
  name    = local.docs_hostname
  type    = "CNAME"
  content = aws_cloudfront_distribution.docs.domain_name
  ttl     = 300
  proxied = false # TLS ends at CloudFront with the ACM certificate
  comment = "docs site (deploy/terraform/corp/corp-portal/docs-site.tf)"
}

# CI's deploy role (deploy.tf) uploads the site and clears CloudFront's cache.
data "aws_iam_policy_document" "ci_deploy_docs" {
  statement {
    actions   = ["s3:ListBucket"]
    resources = [aws_s3_bucket.docs.arn]
  }
  statement {
    actions   = ["s3:PutObject", "s3:DeleteObject"]
    resources = ["${aws_s3_bucket.docs.arn}/*"]
  }
  statement {
    actions   = ["cloudfront:CreateInvalidation"]
    resources = [aws_cloudfront_distribution.docs.arn]
  }
}

resource "aws_iam_role_policy" "ci_deploy_docs" {
  name   = "docs-site"
  role   = aws_iam_role.ci_deploy.id
  policy = data.aws_iam_policy_document.ci_deploy_docs.json
}

# For CI's variables: AWZY_CORP_DOCS_BUCKET and AWZY_CORP_DOCS_DISTRIBUTION_ID.
output "docs_bucket" {
  value = aws_s3_bucket.docs.id
}

output "docs_distribution_id" {
  value = aws_cloudfront_distribution.docs.id
}

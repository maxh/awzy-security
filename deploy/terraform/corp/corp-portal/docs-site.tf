# https://docs.<domain>: awzy's public documentation (docs/site), on Cloudflare Pages. This makes
# the project and its domain; CI uploads each build of main (.github/workflows/docs-site.yml).
# Left out until cloudflare_account_id is set.

locals {
  docs_hostname = "docs.${var.domain}"
  docs          = var.cloudflare_account_id == null ? {} : { site = true }
}

resource "cloudflare_pages_project" "docs" {
  for_each          = local.docs
  account_id        = var.cloudflare_account_id
  name              = "awzy-docs"
  production_branch = "main"
}

resource "cloudflare_pages_domain" "docs" {
  for_each     = local.docs
  account_id   = var.cloudflare_account_id
  project_name = cloudflare_pages_project.docs["site"].name
  name         = local.docs_hostname
}

resource "cloudflare_dns_record" "docs" {
  for_each = local.docs
  zone_id  = local.cf_zone_id
  name     = local.docs_hostname
  type     = "CNAME"
  content  = cloudflare_pages_project.docs["site"].subdomain
  ttl      = 1 # automatic, as a proxied record requires
  proxied  = true
  comment  = "docs site (deploy/terraform/corp/corp-portal/docs-site.tf)"
}

# Deployment-specific inputs for every root module under deploy/terraform/corp, symlinked into
# each. Set them in deployment.env (copy deployment.env.example), then `source ../deployment.env`
# before `tofu init` and `tofu plan`. Nothing here defaults to a particular deployment.

variable "build_account_id" {
  description = "The account that builds and publishes releases, and runs the corp portal for now"
  type        = string
}

variable "build_profile" {
  description = "AWS CLI profile for the build account; null uses the default credentials"
  type        = string
  default     = null
}

variable "cloudflare_account_id" {
  description = "Cloudflare account that holds `domain`, for the docs site on Cloudflare Pages; null leaves the docs site out"
  type        = string
  default     = null
}

variable "cloudflare_token_file" {
  description = "File holding a Cloudflare API token that can edit `domain`'s DNS (and Cloudflare Pages, for the docs site)"
  type        = string
  default     = "~/.config/awzy/cf-token"
}

variable "cloudflare_zone_id" {
  description = "Cloudflare zone ID of `domain`"
  type        = string
}

variable "domain" {
  description = "DNS domain the corp portal and registry live under, e.g. example.com"
  type        = string
}

variable "github_oidc_subject_repo" {
  description = "GitHub's immutable OIDC subject for the repository whose main branch publishes: owner@<owner id>/repo@<repo id>"
  type        = string
}

variable "management_account_id" {
  description = "The AWS Organizations management account"
  type        = string
}

variable "management_profile" {
  description = "AWS CLI profile for the management account; null uses the default credentials"
  type        = string
  default     = null
}

variable "member_accounts" {
  description = "Member accounts of the organization, by key: name and root email (unique per account)"
  type        = map(object({ name = string, email = string }))
  default     = {}
}

variable "region" {
  description = "Region for state and every resource, e.g. us-east-2"
  type        = string
}

variable "state_bucket" {
  description = "S3 bucket for this deployment's Terraform state, in the management account"
  type        = string
}

variable "project" {
  description = "The example client's Google Cloud project for the install"
  type        = string
}

variable "region" {
  description = "The install's region"
  type        = string
  default     = "us-central1"
}

variable "zone" {
  description = "The zone of the install's zonal GKE cluster"
  type        = string
  default     = "us-central1-a"
}

variable "state_bucket" {
  description = "The GCS bucket holding this example's Terraform state"
  type        = string
}

variable "registry_prefix" {
  description = "awzy's registry, as the corp portal registered the install with"
  type        = string
}

variable "demo_hostname" {
  description = "The install's public hostname for the client portal, e.g. gcp.demo.example.com (ADR 0106)"
  type        = string
}

variable "acme_email" {
  description = "The contact Let's Encrypt registers"
  type        = string
}

variable "chart_version" {
  description = "awzy chart versions to follow: by default whatever the install's channel names; or an exact 0.1.0-g<sha> from CI"
  type        = string
  default     = ">=0.0.0-0"
}

variable "github_oidc_subject_repo" {
  description = "GitHub's immutable OIDC subject for the repository whose CI plans and applies these modules (ADR 0115): owner@<owner id>/repo@<repo id>"
  type        = string
  default     = null
}

variable "alert_emails" {
  description = "Who the install's alerts reach (monitoring), by email. Without any, incidents still open in Cloud Monitoring"
  type        = list(string)
  default     = []
}

# Deployment-specific inputs for every root module under examples/example-client, symlinked
# into each. Set them in deployment.env (copy deployment.env.example), then
# `source ../deployment.env` before `tofu init` and `tofu plan`.

variable "acme_email" {
  description = "Contact email for the install's Let's Encrypt account"
  type        = string
}

variable "demo_hostname" {
  description = "The install's public hostname for the client portal, e.g. aws.demo.example.com"
  type        = string
}

variable "demo_redirect_hostnames" {
  description = "Older hostnames that redirect to demo_hostname, e.g. demo.example.com"
  type        = list(string)
  default     = []
}

variable "install_account" {
  description = "Name and root email (unique to it) of the install account"
  type        = object({ name = string, email = string })
}

variable "install_account_id" {
  description = "The member account awzy is installed in"
  type        = string
}

variable "install_profile" {
  description = "AWS CLI profile for the install account; null uses the default credentials"
  type        = string
  default     = null
}

variable "management_account_id" {
  description = "The example client's AWS Organizations management account"
  type        = string
}

variable "management_profile" {
  description = "AWS CLI profile for the management account; null uses the default credentials"
  type        = string
  default     = null
}

variable "region" {
  description = "The install's region, also the only region the guardrail SCP allows, e.g. us-east-2"
  type        = string
}

variable "registry_prefix" {
  description = "Where the install pulls awzy's images and chart, e.g. registry.example.com/awzy (ADR 0028)"
  type        = string
}

variable "state_bucket" {
  description = "S3 bucket for the example client's Terraform state, in its management account"
  type        = string
}

variable "alert_emails" {
  description = "Who the install's alerts reach (monitoring); each confirms an SNS subscription"
  type        = list(string)
  default     = []
}

variable "grafana_admin_user_ids" {
  description = "IAM Identity Center user IDs who administer the install's Grafana (monitoring)"
  type        = list(string)
  default     = []
}

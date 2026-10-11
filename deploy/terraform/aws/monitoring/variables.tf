variable "name" {
  description = "Prefix for everything this creates, e.g. the install's name"
  type        = string
}

variable "network" {
  description = "aws/install's monitoring output: the cluster, and the subnets and security groups its pods are reachable from"
  type = object({
    cluster_name       = string
    subnet_ids         = list(string)
    security_group_ids = list(string)
  })
}

variable "namespaces" {
  description = "The namespaces awzy runs in"
  type        = list(string)
  default     = ["awzy-system", "awzy-portal", "awzy-data"]
}

variable "alert_emails" {
  description = "Who alerts reach: each gets an SNS subscription to confirm"
  type        = list(string)
  default     = []
}

variable "grafana_admin_user_ids" {
  description = "IAM Identity Center user IDs who administer the Grafana workspace"
  type        = list(string)
  default     = []
}

variable "traces" {
  description = "Which traces the collector keeps (ADR 0097): every failed one, every one slower than slow_ms, and sample_percent of the rest"
  type = object({
    slow_ms        = optional(number, 25)
    sample_percent = optional(number, 1)
  })
  default = {}
}

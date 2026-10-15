variable "name" {
  description = "Prefix for what this creates in Cloud Monitoring, e.g. the install's name"
  type        = string
}

variable "services" {
  description = "awzy's services by namespace: each is scraped as its own job, named after it, as the alert rules and dashboards expect"
  type        = map(list(string))
  default = {
    "awzy-system" = [
      "awzy-challenge", "awzy-connectors", "awzy-context", "awzy-gateway", "awzy-intake", "awzy-pdp",
      "awzy-policy", "awzy-reasoner", "awzy-reporter",
    ]
    "awzy-portal" = ["awzy-client-portal", "awzy-dev-idp"]
  }
}

variable "ops_namespace" {
  description = "Where Argo CD runs, whose controller's metrics say whether the release synced"
  type        = string
  default     = "awzy-ops"
}

variable "alert_emails" {
  description = "Who alerts reach, by email"
  type        = list(string)
  default     = []
}

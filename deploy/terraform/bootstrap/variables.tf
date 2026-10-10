variable "install_contract" {
  description = "Output of the per-cloud install module"
  type = object({
    cloud        = string
    region       = string
    size_profile = string
    postgres     = object({ host = string, port = number, database = string, secret_arn = string })
    kafka        = object({ bootstrap_brokers = string })
    storage_class = object({
      name        = string
      provisioner = string
      parameters  = map(string)
    })
    secret_store    = any # External Secrets provider block, passed through as-is
    load_balancer   = object({ service_annotations = map(string) })
    registry_prefix = string
  })
}

variable "chart_version" {
  description = "awzy chart version: an exact release (pinned) or what a channel names"
  type        = string
}

variable "registry_credentials" {
  description = "Pull credentials for registry_prefix: the install ID and its pull token (ADR 0028)"
  type = object({
    username = string
    password = string
  })
  sensitive = true
}

variable "cosign_public_key" {
  description = "PEM public key that release images are signed with (deploy/release/cosign.pub)"
  type        = string
}

variable "client_portal" {
  description = "The client's IdP for the client portal (not needed with demo_idp)"
  type = object({
    issuer     = optional(string, "")
    jwks_url   = optional(string, "")
    public_url = optional(string, "")
  })
  default = {}
}

variable "awzy_bot" {
  description = "Awzy Bot in the client portal (ADR 0048). AWS only: Claude must be enabled in the install account's Bedrock"
  type        = bool
  default     = false
}

variable "investigations" {
  description = "awzy-reasoner's investigations of the people it flags (ADR 0071). AWS only: Claude must be enabled in the install account's Bedrock"
  type        = bool
  default     = false
}

variable "investigation_budget" {
  description = "Investigations' limits (ADR 0071): the most tokens a calendar month spends, and the groups whose members are only ever proposed for containment"
  type = object({
    monthly_tokens    = optional(number, 2000000)
    privileged_groups = optional(list(string), ["it", "security"])
  })
  default = {}
}

variable "llm_model" {
  description = "The Bedrock model ID for Awzy Bot and investigations, when the default (anthropic.claude-opus-5-5) isn't reachable directly, e.g. a cross-region profile like us.anthropic.claude-opus-5-5"
  type        = string
  default     = ""
}

variable "slack" {
  description = "Notifications in your Slack (ADR 0073): the bot token of the Slack app you installed (scopes users:read.email and chat:write) and the security team's channel ID. Null leaves them off"
  type = object({
    token            = string
    security_channel = string
  })
  default   = null
  sensitive = true
}

variable "demo_idp" {
  description = "DEMO INSTALLS ONLY (ADR 0032): sign the client portal in against the dev IdP at https://<hostname>/idp"
  type        = bool
  default     = false
}

variable "demo_data" {
  description = "DEMO INSTALLS ONLY (ADR 0039): provision the synthetic company into awzy-context"
  type        = bool
  default     = false
}

variable "reporter" {
  description = "From registering the install in the corp portal (ADR 0026); null turns the reporter off"
  type = object({
    install_id = string
    token      = string
  })
  default   = null
  sensitive = true
}

variable "ingress" {
  description = "Public hostname of the client portal, and the contact Let's Encrypt registers"
  type = object({
    hostname   = string
    acme_email = string
  })
}

variable "versions" {
  description = "Pinned chart versions of the components bootstrap installs"
  type = object({
    argo_cd             = optional(string, "10.9.7")
    kyverno             = optional(string, "3.8.2")
    external_secrets    = optional(string, "2.11.0")
    clickhouse_operator = optional(string, "0.27.3")
    envoy_gateway       = optional(string, "v1.9.1")
    cert_manager        = optional(string, "v1.21.1")
  })
  default = {}
}

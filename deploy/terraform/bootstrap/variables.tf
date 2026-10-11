variable "install_contract" {
  description = "Output of the per-cloud install module"
  type = object({
    cloud        = string
    region       = string
    size_profile = string
    postgres     = object({ host = string, port = number, database = string, secret_arn = string })
    kafka        = object({ bootstrap_brokers = string, tls = optional(bool, false) })
    # The cloud KMS key awzy-policy signs with, and its public key (ADR 0092).
    policy_signing = object({ kms_key = string, public_key_pem = string })
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
  description = "The awzy chart versions Argo CD follows. The release registry shows an install only the version its release channel (or pinned version) names, as registered in the corp portal, so the default range follows that channel (ADR 0094)"
  type        = string
  default     = ">=0.0.0-0"
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
  description = "The client's IdP for the client portal (not needed with demo_idp): its issuer and keys; the audiences of the portal's own tokens and of those agents get for its MCP endpoint (ADR 0081); and the claim holding a person's SCIM userName, when `sub` doesn't (empty: sub)"
  type = object({
    issuer        = optional(string, "")
    jwks_url      = optional(string, "")
    public_url    = optional(string, "")
    audience      = optional(string, "awzy")
    mcp_audience  = optional(string, "awzy-mcp")
    subject_claim = optional(string, "")
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

variable "mcp_gateway" {
  description = "awzy-gateway, the MCP proxy agents call (ADR 0019): its public hostname (null keeps it in-cluster; it needs a DNS record to the load balancer), and the MCP servers it fronts as the chart's mcpGateway.upstreams. Each tool's tier, and which tiers fail open while the PDP is unreachable, are set in the client portal (ADR 0089). Null leaves it off"
  type = object({
    hostname  = optional(string)
    audience  = optional(string, "awzy-gateway")
    upstreams = any
  })
  default = null
}

variable "intake" {
  description = "awzy-intake, for app and audit-log events: its public hostname for sources outside the cluster (null keeps it in-cluster). Null leaves it off"
  type = object({
    hostname = optional(string)
  })
  default = null
}

variable "drive" {
  description = "The Google Drive connector (ADR 0069): the member user it acts as, an admin to name group shares (optional), the domain (default: the subject's), and the Google service account's JSON key. Null leaves it off"
  type = object({
    subject       = string
    admin_subject = optional(string, "")
    domain        = optional(string, "")
    key_json      = string
  })
  default   = null
  sensitive = true
}

variable "jira" {
  description = "The Jira connector (ADR 0070): the site URL, the account's email, the projects in scope (default: all it can browse), and its API token. Null leaves it off"
  type = object({
    url      = string
    user     = string
    projects = optional(list(string), [])
    token    = string
  })
  default   = null
  sensitive = true
}

variable "demo_idp" {
  description = "DEMO INSTALLS ONLY (ADR 0032): sign the client portal in against the dev IdP at https://<hostname>/idp"
  type        = bool
  default     = false
}

variable "clickhouse_retention_days" {
  description = "How many days ClickHouse keeps decisions and events before they expire; 0 keeps them all. Can change after install"
  type        = number
  default     = 0
  validation {
    condition     = var.clickhouse_retention_days >= 0 && floor(var.clickhouse_retention_days) == var.clickhouse_retention_days
    error_message = "clickhouse_retention_days is a whole number of days, 0 or more."
  }
}

variable "demo_data" {
  description = "DEMO INSTALLS ONLY (ADR 0039): provision the synthetic company into awzy-context"
  type        = bool
  default     = false
}

variable "demo_activity" {
  description = "DEMO INSTALLS ONLY (ADR 0039): the synthetic company's everyday tool calls through awzy-gateway each weekday, so the demo has decisions. Needs demo_data, demo_idp, and mcp_gateway"
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

variable "break_glass_public_keys" {
  description = "PEM public keys of the admins enrolled for break-glass policy rollback (ADR 0022), concatenated; null turns break-glass off"
  type        = string
  default     = null
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

variable "name" {
  description = "Prefix for every resource"
  type        = string
  default     = "awzy"
}

variable "size_profile" {
  description = "small, standard, or large (only small is sized so far)"
  type        = string
  default     = "small"
}

variable "registry_prefix" {
  description = "Where the install pulls awzy images and the chart from: registry.awzysecurity.com/awzy (ADR 0028)"
  type        = string
}

variable "azs" {
  description = "Two availability zones in the install Region"
  type        = list(string)
}

variable "vpc_cidr" {
  type    = string
  default = "10.40.0.0/16"
}

variable "eks_version" {
  description = "Kubernetes version; keep inside EKS standard support"
  type        = string
  default     = "1.36"
}

variable "eks_public_access_cidrs" {
  description = "CIDRs allowed to reach the EKS API endpoint (still IAM-authenticated)"
  type        = list(string)
  default     = ["0.0.0.0/0"]
}

variable "deletion_protection" {
  description = "Protect the databases from terraform destroy; false only for test installs"
  type        = bool
  default     = true
}

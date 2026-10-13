variable "project" {
  description = "The Google Cloud project the install runs in"
  type        = string
}

variable "region" {
  description = "The install's region, e.g. us-central1"
  type        = string
}

variable "zone" {
  description = "The zone the cluster's nodes run in, within region: a zonal cluster, whose management fee GKE's free tier covers"
  type        = string
}

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

variable "subnet_cidr" {
  description = "The nodes' subnet; pods and services get secondary ranges of their own"
  type        = string
  default     = "10.50.0.0/20"
}

variable "gke_master_authorized_cidrs" {
  description = "CIDRs allowed to reach the GKE API endpoint (still IAM-authenticated)"
  type        = list(string)
  default     = ["0.0.0.0/0"]
}

variable "deletion_protection" {
  description = "Protect the cluster and the database from terraform destroy; false only for test installs"
  type        = bool
  default     = true
}

variable "model_region" {
  description = "Where Awzy Bot and investigations call Claude on Vertex AI (ADR 0120): global, which routes to any region with capacity, or a region that offers Claude (e.g. us-east5) to keep requests in it"
  type        = string
  default     = "global"
}

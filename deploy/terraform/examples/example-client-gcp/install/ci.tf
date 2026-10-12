# The identities CI plans and applies this project's modules as (ADR 0115,
# .github/workflows/terraform.yml): install and bootstrap. The example client grants them here, in
# its own project, as a client letting a vendor's pipeline manage a project would: GitHub's OIDC
# tokens, through workload identity federation. Applied by hand once: a pipeline can't create its
# own identity. Off until github_oidc_subject_repo is set.

locals {
  ci = var.github_oidc_subject_repo == null ? {} : {
    # Pull requests and main plan, without refreshing, to list what merging changes.
    plan = {
      subjects = ["ref:refs/heads/main", "pull_request"]
      # Reads the project, the state, and the install's credentials, which bootstrap's plan reads.
      roles = ["roles/viewer", "roles/storage.objectViewer"]
    }
    # Only the example-gcp environment, which waits for a person's approval, applies.
    terraform = {
      subjects = ["environment:example-gcp"]
      roles    = ["roles/owner"]
    }
  }
}

resource "google_project_service" "ci" {
  for_each           = length(local.ci) == 0 ? toset([]) : toset(["iamcredentials.googleapis.com", "sts.googleapis.com"])
  service            = each.key
  disable_on_destroy = false
}

resource "google_iam_workload_identity_pool" "github" {
  count                     = length(local.ci) == 0 ? 0 : 1
  workload_identity_pool_id = "github"
  display_name              = "GitHub Actions"
  depends_on                = [google_project_service.ci]
}

resource "google_iam_workload_identity_pool_provider" "github" {
  count                              = length(local.ci) == 0 ? 0 : 1
  workload_identity_pool_id          = google_iam_workload_identity_pool.github[0].workload_identity_pool_id
  workload_identity_pool_provider_id = "awzy-ci"
  display_name                       = "awzy's CI"
  attribute_mapping                  = { "google.subject" = "assertion.sub" }
  # Only this repository's tokens; GitHub's immutable subject names it by ID.
  attribute_condition = "assertion.sub.startsWith('repo:${var.github_oidc_subject_repo}:')"
  oidc {
    issuer_uri = "https://token.actions.githubusercontent.com"
  }
}

resource "google_service_account" "ci" {
  for_each     = local.ci
  account_id   = "awzy-example-ci-${each.key}"
  display_name = "awzy's CI: ${each.key}"
}

resource "google_project_iam_member" "ci" {
  for_each = merge([for k, c in local.ci : { for r in c.roles : "${k} ${r}" => { ci = k, role = r } }]...)
  project  = var.project
  role     = each.value.role
  member   = google_service_account.ci[each.value.ci].member
}

# Reading the install's credentials is a secret's access, which viewer doesn't give.
resource "google_secret_manager_secret_iam_member" "ci_plan" {
  count     = length(local.ci) == 0 ? 0 : 1
  secret_id = "awzy-install-credentials"
  role      = "roles/secretmanager.secretAccessor"
  member    = google_service_account.ci["plan"].member
}

resource "google_service_account_iam_member" "ci" {
  for_each = merge([for k, c in local.ci : { for s in c.subjects : "${k} ${s}" => { ci = k, subject = s } }]...)
  # Each identity, only to its own subjects' tokens.
  service_account_id = google_service_account.ci[each.value.ci].name
  role               = "roles/iam.workloadIdentityUser"
  member             = "principal://iam.googleapis.com/${google_iam_workload_identity_pool.github[0].name}/subject/repo:${var.github_oidc_subject_repo}:${each.value.subject}"
}

# For CI's variables AWZY_CORP_EXAMPLE_GCP_WIF_PROVIDER, _PLAN_SERVICE_ACCOUNT, and
# _TERRAFORM_SERVICE_ACCOUNT.
output "ci" {
  value = length(local.ci) == 0 ? null : {
    provider         = google_iam_workload_identity_pool_provider.github[0].name
    service_accounts = { for k, sa in google_service_account.ci : k => sa.email }
  }
}

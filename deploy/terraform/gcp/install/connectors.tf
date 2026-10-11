# The connectors' credentials (ADR 0104): one secret each, "unset" until a client admin sets it in
# the client portal (which the portal can't yet do on Google Cloud: it writes to AWS Secrets
# Manager only). External Secrets reads them into the cluster for the connectors; a secret with no
# value at all would fail its ExternalSecret, and with it the release's sync.

resource "google_secret_manager_secret" "connector" {
  for_each  = toset(["drive", "jira"])
  project   = var.project
  secret_id = "${var.name}-connectors-${each.key}"
  replication {
    auto {}
  }
}

resource "google_secret_manager_secret_version" "connector_initial" {
  for_each    = google_secret_manager_secret.connector
  secret      = each.value.id
  secret_data = "unset"
  lifecycle {
    ignore_changes = [secret_data]
  }
}

# The client portal adds versions, and never reads them.
resource "google_secret_manager_secret_iam_member" "portal_writes" {
  for_each  = google_secret_manager_secret.connector
  secret_id = each.value.id
  role      = "roles/secretmanager.secretVersionAdder"
  member    = local.principal.portal
}

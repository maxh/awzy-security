# Claude on Vertex AI for Awzy Bot and investigations (ADR 0120): the API, and leave for the
# portal's and the services' identities to call models in the project. The client still enables
# Claude in Vertex AI's Model Garden; until then calls fail, and both features say so.

resource "google_project_service" "aiplatform" {
  project = var.project
  service = "aiplatform.googleapis.com"
  # Other things in the project may use it.
  disable_on_destroy = false
}

resource "google_project_iam_member" "model_users" {
  for_each = { services = local.principal.services, portal = local.principal.portal }
  project  = var.project
  role     = "roles/aiplatform.user"
  member   = each.value
  # The identity pool exists once the cluster does.
  depends_on = [google_container_cluster.this]
}

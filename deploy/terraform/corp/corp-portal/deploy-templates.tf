# The task definitions Terraform registers are the templates CI deploys from: deploy-corp.sh
# copies the revision named here, sets the image, and points the service at the copy. So a change
# here (environment, secrets, size) reaches the service on the next deploy, and Terraform's own
# revision never drifts from this code. The service itself ignores which revision it runs.
#
# The image in a template is only what a brand-new service starts with; CI replaces it.

resource "aws_ssm_parameter" "task_definition_template" {
  for_each = merge(
    { "awzy-corp-portal" = aws_ecs_task_definition.corp.arn },
    local.registry_enabled ? { "awzy-registry" = aws_ecs_task_definition.registry[0].arn } : {},
  )
  name        = "/awzy-corp-portal/task-definition-template/${each.key}"
  description = "Task definition CI deploys ${each.key} from (deploy/terraform/corp/corp-portal)"
  type        = "String"
  value       = each.value
}

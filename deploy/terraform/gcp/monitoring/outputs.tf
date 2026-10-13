output "alert_policies" {
  description = "The alert policies, by rule name"
  value       = { for name, p in google_monitoring_alert_policy.rule : name => p.name }
}

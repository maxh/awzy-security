# awzy-connectors · ingest jobs

Pull-based sync from systems of record into [awzy-context](../context/README.md). Part of
[awzy-authz](../../README.md).

Sources: IdP (Okta, Entra ID, Google Workspace), HR (Workday), work tracking (Jira, Linear), code
hosts, data catalogs, Google Drive or SharePoint metadata. One crate per connector behind a
shared trait.

- Runs as Kubernetes CronJobs plus webhooks
- Writes raw records to object storage and normalized entities to awzy-context

A skeleton so far ([platform plan](../../../docs/platform-plan.md#built-so-far)).

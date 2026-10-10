# awzy-connectors · ingest jobs

Pull-based sync from systems of record into [awzy-context](../context/README.md). Part of
[awzy-authz](../../README.md).

Sources: IdP (Okta, Entra ID, Google Workspace), HR (Workday), work tracking (Jira, Linear), code
hosts, data catalogs, Google Drive or SharePoint metadata. One crate per connector behind a
shared trait.

- Runs as Kubernetes CronJobs plus webhooks
- Writes raw records to object storage and normalized entities to awzy-context

## Google Drive (ADR 0069)

Shared drives' files, folders, and sharing, as resources from source `google_drive`, with IDs
`gdrive:<Drive ID>`. Read-only metadata: no content. A full crawl on start, then each drive's
change feed every interval; resources that left Drive are deleted.

| Variable | |
| --- | --- |
| `AWZY_CONNECTORS_DRIVE_ADMIN_SUBJECT` | An admin, to name group shares as SCIM does. Optional |
| `AWZY_CONNECTORS_DRIVE_DOMAIN` | The client's domain. Default: the subject's |
| `AWZY_CONNECTORS_DRIVE_INTERVAL_SECS` | Between change rounds. Default 300 |
| `AWZY_CONNECTORS_DRIVE_KEY_FILE` | The Google service account's JSON key |
| `AWZY_CONNECTORS_DRIVE_SUBJECT` | The member user it acts as. Setting it turns Drive on |
| `AWZY_CONTEXT_URL` | awzy-context, for writes |

In Google: a service account with domain-wide delegation for
`https://www.googleapis.com/auth/drive.metadata.readonly` and
`https://www.googleapis.com/auth/admin.directory.group.readonly`, and a user added as a Content
manager to each shared drive awzy should cover. `/metrics` reports
`awzy_connectors_drive_sync_age_seconds` and the resources written and deleted.

Other sources aren't built yet ([platform plan](../../../docs/platform-plan.md#built-so-far)).

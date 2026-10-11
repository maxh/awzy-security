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

## Jira (ADR 0070)

Jira Cloud issues as work items from source `jira`, with IDs `jira:<issue ID>`: the assignee by
email, and the Google Drive files the issue cites (in its description or remote links) as the
resources it needs, so `context.assigned` works with Drive. Done issues are `closed`. A full crawl
of open issues on start and daily, which deletes what it no longer sees; between them, issues
updated since the last round.

| Variable | |
| --- | --- |
| `AWZY_CONNECTORS_JIRA_INTERVAL_SECS` | Between rounds. Default 300 |
| `AWZY_CONNECTORS_JIRA_PROJECTS` | Project keys, comma-separated. Default: every project the account can browse |
| `AWZY_CONNECTORS_JIRA_TOKEN_FILE` | The account's Atlassian API token |
| `AWZY_CONNECTORS_JIRA_URL` | `https://<site>.atlassian.net`. Setting it turns Jira on |
| `AWZY_CONNECTORS_JIRA_USER` | The account's email |

The account needs only to browse the projects in scope. Assignees match people when SCIM user
names are emails and Jira shows the account their emails (the Atlassian organization's email
visibility); a hidden one is counted in `unmapped_assignees`.

Other sources aren't built yet ([platform plan](../../../docs/plans/platform-plan.md#built-so-far)).

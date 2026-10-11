# awzy-connectors · ingest jobs

Pull-based sync from systems of record into [awzy-context](../context/README.md). Part of
[awzy-authz](../../README.md).

Sources: IdP (Okta, Entra ID, Google Workspace), HR (Workday), work tracking (Jira, Linear), code
hosts, data catalogs, Google Drive or SharePoint metadata. One crate per connector behind a
shared trait.

- One long-running process; a supervisor reads each connector's settings from awzy-context (as
  the client portal sets them) and its credential from a mounted file every minute, and starts,
  restarts, or stops the connector to match (ADR 0104)
- Writes normalized entities to awzy-context

| Variable | |
| --- | --- |
| `AWZY_CONNECTORS_DRIVE_KEY_FILE` | Where Drive's credential is mounted. Default `/etc/awzy/drive/key.json` |
| `AWZY_CONNECTORS_JIRA_TOKEN_FILE` | Where Jira's credential is mounted. Default `/etc/awzy/jira/token` |
| `AWZY_CONTEXT_URL` | awzy-context, for settings and writes |

## Google Drive (ADR 0069)

Shared drives' files, folders, and sharing, as resources from source `google_drive`, with IDs
`gdrive:<Drive ID>`. Read-only metadata: no content. A full crawl on start, then each drive's
change feed every interval; resources that left Drive are deleted.

Settings (`/v1/connector-settings/drive` in awzy-context): `subject`, the member user it acts as;
`admin_subject`, an admin to name group shares as SCIM does (optional); `domain`, the client's
domain (default: the subject's); `interval_secs` (default 300). Credential: the Google service
account's JSON key.

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

Settings (`/v1/connector-settings/jira`): `url`, `https://<site>.atlassian.net`; `user`, the
account's email; `projects`, project keys (default: every project it can browse); `watchers`,
whether an issue's watchers count as working on it, at one more Jira call per issue (default
off); `interval_secs` (default 300). Credential: the account's Atlassian API token.

The account needs only to browse the projects in scope. Assignees match people when SCIM user
names are emails and Jira shows the account their emails (the Atlassian organization's email
visibility); a hidden one is counted in `unmapped_assignees`.

Other sources aren't built yet ([platform plan](../../../docs/plans/platform-plan.md#built-so-far)).

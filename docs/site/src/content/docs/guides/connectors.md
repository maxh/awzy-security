---
title: Connectors
description: Bringing your shared drives and your Jira work into awzy, so decisions know what a file is and who's working on it.
---

awzy decides better when it knows what a resource is and why someone would need it. Connectors
read that from your systems of record and keep awzy-context up to date. They read metadata only:
names, folders, sharing, and assignments, never a file's content. Two are built: Google Drive and
Jira. Both are off until you configure them in bootstrap.

## What they add to decisions

- **Google Drive** brings each shared drive's files and folders in as resources, with their
  sharing. Policies can then tell a file shared with everyone from one shared with a single team,
  and awzy-context classifies each file's sensitivity where its rules can.
- **Jira** brings in open issues as work: who they're assigned to, and the Drive files each issue
  cites in its description or links. When a check's resource is linked from work assigned to the
  principal, the PDP sets `context.assigned`. The built-in policies use it to tell ordinary work
  from reaching outside it: a contractor, or an AI agent acting for someone, reaching confidential
  data that none of their work links needs approval.

Without Jira, `context.assigned` is absent, and those policies don't apply.

## Google Drive

**In Google Workspace:**

1. Create a Google Cloud service account and a JSON key for it.
2. In the Admin console, give the service account domain-wide delegation for these scopes:
   - `https://www.googleapis.com/auth/drive.metadata.readonly`
   - `https://www.googleapis.com/auth/admin.directory.group.readonly`
3. Pick a member user for the connector to act as, and add them as a **Content manager** of each
   shared drive awzy should cover. Drives they aren't in stay out of awzy.
4. Optionally, pick an admin user, so group shares are named the way your IdP names groups.

**In bootstrap**, set `drive`:

| Field | |
| --- | --- |
| `subject` | The member user from step 3 |
| `admin_subject` | The admin from step 4. Optional |
| `domain` | Your domain. Default: the subject's |
| `key_json` | The service account's JSON key |

The connector crawls every covered drive when it starts, then follows each drive's changes every
five minutes. Files that leave Drive leave awzy-context.

## Jira

**In Jira Cloud:** use an account that can browse the projects awzy should see, and create an
Atlassian API token for it. Assignees are matched to people by email, so the account must be able
to see users' emails (your Atlassian organization's email visibility). This also assumes your
IdP's user names are email addresses.

**In bootstrap**, set `jira`:

| Field | |
| --- | --- |
| `url` | `https://<your-site>.atlassian.net` |
| `user` | The account's email |
| `projects` | Project keys to cover. Default: every project the account can browse |
| `token` | The account's API token |

The connector reads open issues when it starts and again daily, and issues updated since its last
round every five minutes. Done issues no longer count as work.

## Checking that they work

awzy-connectors serves Prometheus metrics. `awzy_connectors_drive_sync_age_seconds` says how long
ago Drive last synced. The Jira connector counts assignees it couldn't match to a person
(`unmapped_assignees`); a growing count usually means hidden emails.

Other sources, such as Okta, Workday, and code hosts, aren't built yet.

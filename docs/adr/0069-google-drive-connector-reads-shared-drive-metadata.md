# 0069. The Google Drive connector reads shared drives' metadata and sharing, read-only

- Status: Accepted
- Date: 2026-10-10
- Implements part of: [0044](0044-pdp-loads-resources-from-awzy-context.md) (resources come from
  connectors) and [0061](0061-ownership-and-sharing-in-the-resource-entity.md) (sharing on the
  resource)

## Context

Every resource awzy decides about comes from the synthetic company. Phase 2 of the platform plan
is done only when policies see team, work assignment, and sensitivity "kept current without
manual entry", which takes a connector to a real system. Google Drive is the first: it maps onto
`Resource` directly (folders, sharing with people, groups, the domain, or anyone), and installs on
Google Workspace already provision people and groups over SCIM (0031), so the IDs line up.

Three things about Drive shape the design. Its file listing doesn't include permissions for
files in shared drives; those come from the drive's members, plus per-file permissions that Drive
flags with `hasAugmentedPermissions`. It shares with groups by email, while awzy names a group by
its display name (0031). And awzy-context reads any `domain` share as "everyone", which is right
only for the client's own domain.

## Decision

- **Shared drives first.** The connector crawls every shared drive its account is a member of.
  Each drive is a `Resource` of kind `shared_drive`, holding its members as sharing; files and
  folders hang under it by `parent`, so they inherit the members (0061), and a file's own
  permissions are read only when Drive says it has some. Users' My Drives come later: covering
  them means acting as each user in turn.
- **Metadata only, read-only.** Names, types, folders, sharing, and links, under the
  `drive.metadata.readonly` scope. No content: the classifier works from names and folders until a
  separate decision adds excerpts, which need `drive.readonly`. Whatever role the account has in a
  drive, the scope means it can't change anything.
- **A Google service account with domain-wide delegation**, acting as a dedicated user that's a
  member of each drive to cover (`AWZY_CONNECTORS_DRIVE_SUBJECT`). Drive shows sharing only to
  members who can see it, so that user should be a Content manager. Its key is a JSON file
  (`AWZY_CONNECTORS_DRIVE_KEY_FILE`) from the install's secret store. Keyless federation from the
  install's cloud to Google comes later.
- **Groups by name.** With `AWZY_CONNECTORS_DRIVE_ADMIN_SUBJECT` (an admin, under
  `admin.directory.group.readonly`), the connector reads the domain's groups and writes a group
  share under the group's name, as SCIM provisions it. Without it, a group share keeps the email,
  and matches only if groups are provisioned under their emails.
- **Only the client's domain is everyone.** A `domain` share for `AWZY_CONNECTORS_DRIVE_DOMAIN`
  becomes a `domain` share; one for any other domain is left out of sharing and counted in the
  resource's `external_shares` attribute, as are shares with people outside the domain.
- **Roles:** reader and commenter read; writer, file organizer, and organizer write (0061's
  `writer_*`); owner is owner.
- **IDs:** `gdrive:<Drive ID>`, from source `google_drive`. A shared drive's root folder has the
  drive's own ID, so files at the top point at the drive.
- **Sync:** a full crawl when the connector starts, then Drive's change feed per drive every
  `AWZY_CONNECTORS_DRIVE_INTERVAL_SECS` (default 300), with members re-read each round. A removed
  or trashed file is deleted from awzy-context. After a full crawl, any `google_drive` resource the
  crawl didn't see is deleted, so a restart also catches what was removed while it was down. The
  change cursors live in memory; a restart costs one full crawl.
- **Writes** go through awzy-context's batch API (0044), 1,000 at a time, with the connector's
  service token (0055), as `AwzyService::awzy-connectors`.

Rejected: listing every file's permissions. It's one request per file, against Drive's per-user
quota, to learn what membership and `hasAugmentedPermissions` already say. Rejected: crawling as
a domain admin with `useDomainAdminAccess`. Drive grants that only for managing drives, not for
listing their files.

## Consequences

- A client turns it on with a Google service account, delegation for two read-only scopes, and a
  member user added to the drives awzy should see. Drives it isn't added to stay invisible, which
  is also how a client keeps one out.
- Sensitivity comes from the classifier's name and folder rules until labels (Drive Labels) or
  excerpts are read.
- A file shared outside the company carries `external_shares` in its attributes. Policies don't see
  resource attributes yet; making it one is a schema change for later.

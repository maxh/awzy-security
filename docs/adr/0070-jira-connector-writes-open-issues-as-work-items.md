# 0070. The Jira connector writes issues as work items, linked to the Drive files they cite

- Status: Accepted
- Date: 2026-10-10
- Implements part of: [0046](0046-work-assignment-as-context-assigned.md) (work assignment as
  `context.assigned`)

## Context

`context.assigned` (0046) tells a policy whether the accessor is on open work that links the
resource. Its work items come only from the synthetic company, so on a real install it's never
set. Phase 2 of the platform plan needs work assignment kept current without manual entry.

Jira is where most of that work lives. An issue has an assignee and, in its description or its
links, the documents the work needs. Since 0069, those documents can be resources in
awzy-context. Two things about Jira Cloud shape the design. It names people by an opaque account
ID and shows their email only where the person's profile, or the organization, allows it. And
deleted issues don't show up in a search for what changed.

## Decision

- **Every issue in scope becomes a work item**, `jira:<issue ID>` from source `jira`, of kind
  `ticket`. The title is `<KEY>: <summary>`; the key, type, and project go in its attributes. An
  issue in Jira's Done status category is `closed`, so it stops counting (0046); anything else is
  `open`.
- **Assignees by email.** The assignee's email, lowercased, is the person's ID, which matches when
  SCIM user names are emails (as Google Workspace and most Okta setups provision them). An
  assignee whose email Jira hides isn't guessed at: the work item gets no assignee and counts it
  in `unmapped_assignees`. Organizations make emails visible to the connector's account in their
  Atlassian settings.
- **Linked resources are the Google Drive files the issue cites**, in its description or its
  remote links, as `gdrive:<ID>` (0069). Other links are counted, not followed.
- **Scope** is the projects in `AWZY_CONNECTORS_JIRA_PROJECTS`, or every project the account can
  browse.
- **Sync:** a full crawl of open issues on start and once a day, after which any `jira` work item
  the crawl didn't see is deleted. That catches issues deleted, moved out of scope, or closed
  while the connector was down. In between, every `AWZY_CONNECTORS_JIRA_INTERVAL_SECS` (default
  300), a search for issues updated since the last round, closed ones included, so a closed issue
  stops counting within one round.
- **Read-only access** as a dedicated Atlassian account with an API token
  (`AWZY_CONNECTORS_JIRA_USER`, `AWZY_CONNECTORS_JIRA_TOKEN_FILE`), which needs only to browse the
  projects in scope.
- **awzy-context lists work items by source**, so the crawl can find what it didn't see.

Rejected: resolving hidden emails through Atlassian's organization admin API. It needs an
organization admin key, far more than reading issues, for something the organization's own
visibility setting already solves. Rejected: following every link. Only Drive files are resources
in awzy-context so far; the rest would be IDs that match nothing.

## Consequences

- With Drive and Jira both connected, "Sam is assigned to the ticket that links this
  spreadsheet" is something a policy can rely on.
- Watchers, Confluence pages, repositories, and other trackers aren't linked yet. They come with
  the connectors that make them resources.
- An issue's description is read to find links, and only the links are kept.

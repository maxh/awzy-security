---
title: Domain model
description: The entities awzy decides about, the records it keeps, and how IDs and times are written.
---

awzy works with two kinds of thing, and names them so they can't be confused:

- **Entities from your systems:** people, groups, service accounts, documents, MCP tools. awzy
  didn't create them, so it keeps the IDs your IdP and apps give them. They're written as a type
  and an ID, `Human::"alice"`, and fields holding their IDs end in `_id` (`human_id`).
- **awzy's own records:** decisions, policy bundles, challenges, containments. awzy creates them
  and gives each a [QID](#qids). Fields holding one end in `_qid` (`decision_qid`).

## Entities

These are the Cedar entity types your policies see. awzy-context keeps them up to date from your
IdP (over SCIM) and from connectors to your apps.

| Entity | What it is | Attributes policies can use |
| --- | --- | --- |
| `Human` | A person, by your IdP's ID for them | `active`; member of `Group`s |
| `ServiceAccount` | Anything non-human with its own credentials: a backend service, a scheduled job, an AI agent | `owner` (the accountable `Human`), `ai` |
| `Group` | A group or team from your IdP. Groups can nest | |
| `Resource` | Something people access: a document, a folder, a project. Resources nest | `sensitivity`, `unclassified`, `owner`, who it's shared with |
| `McpServer`, `McpTool` | An MCP server behind awzy-gateway, and its tools (`<server>/<tool>`) | `sensitivity` |
| `PortalArea` | An area of the client portal: `decisions`, `policies`, `enforcement`, `directory`, `challenges` | `sensitivity` |

`Human` and `ServiceAccount` are the **accessors**: the subject of every check. See
[People, agents, and delegation](/concepts/principals-and-delegation/).

**Sensitivity** is one of `public`, `internal`, `confidential`, or `crown_jewels`. A resource
that no label or classifier has set is marked `unclassified` and gets the default tier you
choose, so policies can treat it with care.

**Actions** are `read` and `write` on a `Resource`, `call` on an `McpTool`, `view` and
`administer` on a `PortalArea`, and `approve` for approving someone else's challenged request.

## Records

| Record | QID prefix | What it is |
| --- | --- | --- |
| Decision | `decision_` | One verdict on one check, with the policies that produced it. See [How awzy decides](/concepts/how-awzy-decides/) |
| Policy bundle | `policy_bundle_` | A signed, versioned set of your Cedar policies. See [Writing policies](/guides/writing-policies/) |
| Challenge | `challenge_` | What an accessor must pass before retrying: a justification, a security key, or an approval |
| Containment | `containment_` | An accessor whose access is revoked until someone lifts it. See [Challenges and containment](/concepts/challenges-and-containment/) |
| Security key | `security_key_` | A person's registered WebAuthn key, used to answer verification challenges |
| Event | `event_` | One thing an accessor did, as an app or cloud audit log recorded it |

All of them stay in your cloud account.

## Who did something

Wherever awzy records who did something (published a bundle, lifted a containment, approved a
request), it records an **actor**: an entity, and the person it acted for, if any. The portal
shows it as `alice`, or `sales-genie for alice`. An actor is one of:

| Type | Who |
| --- | --- |
| `Human` | A person, by your IdP's ID |
| `ServiceAccount` | One of your service accounts, usually acting for a person |
| `AwzyService` | One of awzy's own components, such as `awzy-reasoner` containing a risky accessor, or `awzy-bot` drafting a policy for someone |
| `BreakGlassKey` | A break-glass key, by its fingerprint. Who held it isn't known |

In the APIs, an actor is `{subject, on_behalf_of}`, with each part a type and ID.

## QIDs

Every record awzy creates has a QID: its type, an underscore, and a
[UUIDv7](https://www.rfc-editor.org/rfc/rfc9562#name-uuid-version-7).

```text
decision_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e90
```

- **It says what it identifies.** A QID in a log line, a ticket, or a chat message can't be
  mistaken for some other record's ID, or for an ID from your own systems. Passing a challenge's
  QID where a decision's is expected is rejected, and so is a bare UUID.
- **It sorts by time.** The UUID starts with its creation time, so QIDs sort in the order the
  records were made.
- **It's the same everywhere.** gRPC, the HTTP decision API, the portal's GraphQL API, MCP tool
  arguments, and logs all use the QID. Fields that hold one are named `…_qid` (`decision_qid`,
  `challengeQid` in GraphQL); a record's own is `qid`. GraphQL objects also have an `id`, which
  Relay clients use to cache them.

## Timestamps

Every time awzy stores or sends is in UTC, in a type that says so.

| Where | Form | Example |
| --- | --- | --- |
| GraphQL (client portal) | The `UtcTimestamp` scalar: RFC 3339 with milliseconds and `Z`. Input with another offset is converted to UTC | `2026-10-09T01:33:02.172Z` |
| gRPC | `google.protobuf.Timestamp` | |
| The decision and event logs | `at_ms`: milliseconds since the Unix epoch | `1791509582172` |
| Inside a signed request, such as break-glass | `…_unix`: seconds since the Unix epoch, so the signed bytes are unambiguous | `1791509582` |

Field names end in `_at` (`created_at`, `expires_at`), or `_at_ms` and `_at_unix` for the
integer forms. The portal shows times in the viewer's own time zone; everything underneath stays
in UTC.

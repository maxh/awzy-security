---
title: API conventions
description: "How awzy's APIs write IDs, QIDs, actors, times, and the context of a check."
---

The same conventions hold across gRPC, the HTTP decision API, the client portal's GraphQL API, MCP
tool arguments, and logs. The ideas behind them are in the [domain model](/concepts/domain-model/).

## IDs

- **Your entities' IDs** are the ones your IdP and apps give them. An entity is written as a type
  and an ID, `Human::"alice"` in Cedar, `{"type": "Human", "id": "alice"}` in JSON, and fields
  holding one end in `_id` (`human_id`).
- **awzy's records** are identified by a QID, below. Fields holding one end in `_qid`.

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

## Actors

Over gRPC, an actor is `{subject, on_behalf_of}`, with each part a type and ID. The client
portal's GraphQL API gives its `type` and `id`, and the person it acted for. See
[who did something](/concepts/domain-model/#who-did-something) for the types.

## Request context

Every check carries a context. Most of it awzy sets from what it knows. A caller that puts
`human_present`, `on_behalf_of`, `assigned`, or `automation_score` in the context is rejected, so they can't be
claimed; the first two come from the request's own fields and its validated token.

| Field              | Type               | Set by                | What it is                                                                                                                                                               |
| ------------------ | ------------------ | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `risk_score`       | `Long`             | awzy                  | The principal's risk, 0 to 100. awzy-reasoner's score replaces whatever the caller sent; before it has scored the principal, the caller's value or `0`                   |
| `human_present`    | `Bool`             | awzy                  | Whether a person is there to answer a challenge now: yes for a `Human`, no for a service account unless the enforcement point says so                                    |
| `on_behalf_of`     | `Human`, optional  | awzy                  | The person a service account acts for, from its validated token. Absent when it acts on its own authority                                                                |
| `automation_score` | `Long`, optional   | awzy                  | For a person: 0 to 100, how much their recent requests look like software using their credentials. Absent until awzy-reasoner has scored them                            |
| `assigned`         | `Bool`, optional   | awzy                  | Whether the principal is on open work that links the resource. Absent when awzy doesn't know                                                                             |
| `client`           | `String`, optional | The enforcement point | The calling software, such as `browser`, `cli`, or `mcp` (awzy-gateway sets `mcp`). A `Human` arriving through `mcp` is software holding a person's own token            |
| `mcp_client`       | `String`, optional | awzy-gateway          | The MCP client's name, as its session's `initialize` gave it, such as `claude-code`. What the client says it is: a hint, not proof                                       |
| `approving_for`    | `Human`, optional  | awzy-challenge        | Only on an `approve` check: the person who answers for the challenged request. An agent never approves for its own owner, and without this field it can't approve at all |

When an agent acts for a person, the check runs twice, once as each, and each sees its own
`risk_score` and `assigned`. See [How awzy decides](/concepts/how-awzy-decides/).

## Timestamps

Every time awzy stores or sends is in UTC, in a type that says so.

| Where                                        | Form                                                                                                         | Example                    |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------ | -------------------------- |
| GraphQL (client portal)                      | The `UtcTimestamp` scalar: RFC 3339 with milliseconds and `Z`. Input with another offset is converted to UTC | `2026-10-09T01:33:02.172Z` |
| gRPC                                         | `google.protobuf.Timestamp`                                                                                  |                            |
| The decision and event logs                  | `at_ms`: milliseconds since the Unix epoch                                                                   | `1791509582172`            |
| Inside a signed request, such as break-glass | `…_unix`: seconds since the Unix epoch, so the signed bytes are unambiguous                                  | `1791509582`               |

Field names end in `_at` (`created_at`, `expires_at`), or `_at_ms` and `_at_unix` for the
integer forms. The portal shows times in the viewer's own time zone; everything underneath stays
in UTC.

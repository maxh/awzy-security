---
title: Services
description: What runs in your install, where each piece keeps its data, and the APIs each one serves.
---

An install is a handful of Rust services in your Kubernetes cluster, installed together by one
Helm chart. They keep state in your managed Postgres, Kafka, and ClickHouse, and nothing in them
keeps state of its own on disk. All of them run in your cloud account; see the
[security model](/reference/security-model/).

## At a glance

| Service | What it does | Keeps data in | Serves |
| --- | --- | --- | --- |
| [awzy-pdp](#awzy-pdp) | Decides every check, in milliseconds | Nothing durable; logs decisions to Kafka | gRPC `Authorizer`, HTTP decision API |
| [awzy-gateway](#awzy-gateway) | Checks every MCP tool call before it runs | Nothing durable | MCP proxy |
| [awzy-policy](#awzy-policy) | Keeps, signs, and serves policy bundles and enforcement modes | Postgres | gRPC `PolicyService` |
| [awzy-context](#awzy-context) | Keeps what awzy knows about your organization | Postgres | HTTP API, SCIM 2.0 |
| [awzy-challenge](#awzy-challenge) | Keeps challenges, containments, and security keys; tells people what's waiting | Postgres | gRPC `ChallengeService` |
| [awzy-intake](#awzy-intake) | Takes in app and cloud audit logs | Kafka | HTTP event API |
| [awzy-reasoner](#awzy-reasoner) | Scores risk, contains, and investigates | Nothing of its own; writes to awzy-context and awzy-challenge | Health and metrics only |
| [awzy-connectors](#awzy-connectors) | Brings in resources and work items from your systems | Nothing of its own; writes to awzy-context | Health and metrics only |
| [Client portal](#client-portal) | Your team's web app and MCP endpoint | Nothing of its own | GraphQL, MCP |
| [awzy-reporter](#awzy-reporter) | Sends the install's heartbeat | Nothing | Health and metrics only |

Every service serves its APIs and these on one port, 8080:

- `/healthz`: the process is up.
- `/readyz`: it can serve, such as a PDP that has loaded its policies and the containment list.
- `/version`: its release.
- `/metrics`: Prometheus metrics, for dashboards and alerts.

Inside the install, services prove who's calling with Kubernetes service-account tokens, and
each API allows only the callers that need it.

## Where data lives

### Postgres

Three databases on your managed Postgres (Aurora on AWS). Each service migrates its own on start.

| Database | Owner | Tables |
| --- | --- | --- |
| `awzy_policy` | awzy-policy | `policy_bundles`, `enforcement_mode_changes` (append-only), `break_glass_events` |
| `awzy_context` | awzy-context | `humans`, `removed_humans`, `groups`, `group_members`, `service_accounts`, `resources`, `resource_changes`, `work_items`, `risk_scores`, `investigations`, `snapshot_version` |
| `awzy_challenge` | awzy-challenge | `challenges`, `containments`, `containment_feed`, `security_keys`, `webauthn_ceremonies`, `notifications` |

### Kafka

| Topic | Written by | Read by |
| --- | --- | --- |
| `awzy.decisions.v1` | awzy-pdp, one record per decision, keyed by principal | ClickHouse |
| `awzy.events.v1` | awzy-intake, one record per event | ClickHouse |

### ClickHouse

Runs in your cluster through its operator. Its Kafka-engine tables read both topics into
`decisions` and `events`, the history that the client portal searches and awzy-reasoner scores.
The `awzy-clickhouse-migrate` job applies its schema on each release, and the install's retention:
the chart's `clickhouse.retentionDays` sets how many days decisions and events are kept (0, the
default, keeps them all), and can change at any time. Both tables are also kept in resource order,
for "who touched this resource". awzy-reasoner and
awzy-reporter each read it as their own read-only user.

### Outside services, only when you turn them on

- **Your cloud's model service** (Bedrock on AWS): Awzy Bot in the client portal, and
  investigations in awzy-reasoner.
- **Your Slack and your mail relay:** awzy-challenge's notifications.
- **Your Google Drive and Jira:** awzy-connectors reads them.

## The services

### awzy-pdp

The policy decision point: evaluates your Cedar policies for each check.

- **APIs:** gRPC `awzy.authz.v1.Authorizer/Check`, and the same decision over HTTP at
  `POST /v1/access/evaluation` (AuthZEN-style). See the [decision API](/reference/decision-api/).
- **Keeps in memory, and reloads:**
  - the active policy bundle and enforcement mode, from awzy-policy;
  - people, groups, service accounts, and risk scores, from awzy-context's snapshot;
  - who is contained, streamed from awzy-challenge;
  - resources, fetched from awzy-context and cached until awzy-context says they changed.
- **Writes:** every decision to `awzy.decisions.v1`. When a challenge needs opening, it calls
  awzy-challenge's `Resolve`.

### awzy-gateway

An enforcement point in front of your MCP servers.

- **API:** `/mcp/{upstream}`, a Model Context Protocol proxy. Every `tools/call` is checked as
  `call` on the tool, and as `read` or `write` on the resource it names, before it's forwarded.
  A blocked call comes back as a tool error that says why, with the decision's QID.
- **Calls:** awzy-pdp for each tool call. It follows awzy-challenge's containment feed, so during
  a PDP outage it never replays an allow for someone contained.
- **Writes:** each tool call it forwards to awzy-intake, as an event, when awzy-intake is on.

### awzy-policy

Policy bundles and enforcement modes.

- **API:** gRPC `awzy.policy.v1.PolicyService`:
  - `CreateDraft`, `Publish`, `GetActive`, `ListBundles`, `GetBundle`: bundles, validated before
    they're stored and signed when they're published;
  - `GetEnforcement`, `SetEnforcement`, `ListEnforcementChanges`: the install's mode, and the mode
    for automated containments;
  - `GetOutageFallback`, `SetOutageFallback`: each MCP tool's tier, and which tiers fail open
    while the PDP is unreachable;
  - `BreakGlassRollback`: a rollback signed with an admin's break-glass key.
- **Stores:** Postgres `awzy_policy`. Signs with your cloud's key service (KMS on AWS), as its own service account.
- **Called by:** each PDP, which waits on it for a new bundle or mode, so a publish reaches every PDP as it's made; the client portal; and
  awzy-reasoner, which reads the automated-containments mode; and awzy-gateway, which reads the
  outage fallback.

### awzy-context

What awzy knows about your organization.

- **APIs (HTTP):**
  - SCIM 2.0 at `/scim/v2/` (`Users`, `Groups`, `ServiceProviderConfig`), for your IdP, with its
    own bearer token;
  - `GET /v1/principals`: the snapshot of people, groups, service accounts, and risk scores that
    PDPs load;
  - `/v1/resources` and `/v1/work-items`, with a `batch` write for each: written by connectors,
    read by PDPs by ID;
  - `/v1/service-accounts` and `/v1/directory/…`: the client portal's Directory page;
  - `POST /v1/risk`: awzy-reasoner's scores;
  - `/v1/investigations`: awzy-reasoner records investigations and reads its token usage, and the
    client portal lists them and closes them (`POST /v1/investigations/{qid}/close`).
- **Stores:** Postgres `awzy_context`.
- **In the background:** classifies new and changed resources by their labels, content markings,
  names, and folders.

### awzy-challenge

Challenges, containments, security keys, and notifications.

- **API:** gRPC `awzy.challenge.v1.ChallengeService`:
  - `Resolve`: the PDP opens or finds the challenge for a request;
  - `GetChallenge`, `ListChallenges`, `Justify`, `Decide`: answering challenges, through the client
    portal;
  - `WatchChallenges`: a stream of challenges as they open and change, for the portal's live
    Challenges page;
  - `Contain`, `Lift`, `ListContainments`: containments, from the client portal and awzy-reasoner;
  - `WatchContainments`: a stream of who is contained, with a heartbeat, followed by every PDP and
    gateway;
  - `ListSecurityKeys`, `BeginKeyRegistration`, `FinishKeyRegistration`, `BeginKeyChange`,
    `RemoveSecurityKey`, `ResetSecurityKey`, `BeginVerification`, `FinishVerification`: security
    keys (WebAuthn), through the client portal.
- **Stores:** Postgres `awzy_challenge`. Notifications are recorded first and sent by a worker to
  your Slack and by email, each with its own retries.
- **Calls:** awzy-pdp, to check that an approver may approve.

### awzy-intake

Audit logs from your apps and cloud, as events.

- **API (HTTP):** `POST /v1/events` for app events in awzy's own schema, and one endpoint per
  cloud audit log format (`POST /v1/events/aws-cloudtrail` for AWS CloudTrail so far). All take
  your log shippers' token.
- **Writes:** each event, normalized, to `awzy.events.v1`, acknowledged once Kafka has it.
- **People in cloud audit logs:** a cloud role session that names a person is recorded as that
  person, so it counts toward their risk score. On AWS, that's a session of an IAM Identity Center
  role, or of a role named in the chart's `intake.humanRoles`; the original ARN stays with the
  event.

### awzy-reasoner

The slow path. Every 15 minutes it:

1. reads the last 30 days of decisions and events from ClickHouse;
2. writes each person's and service account's risk score to awzy-context;
3. contains people when the day's evidence adds up, through awzy-challenge, under the
   automated-containments mode it reads from awzy-policy;
4. when you've turned them on, investigates the people it flags, with your model service, and
   records each investigation in awzy-context.

- **API:** none beyond health and metrics. It runs as a single replica.
- **Stores:** nothing of its own.

### awzy-connectors

Your systems of record, as resources and work items.

- **Google Drive:** every shared drive its account can see, with files, folders, and sharing.
- **Jira:** open issues, as work items, linked to the Drive files they cite.
- **Writes:** to awzy-context's resource and work-item APIs.
- **API:** none beyond health and metrics.

### Client portal

Your team's web app, served with its own frontend. See [The client portal](/client-portal/) for
what each page does.

- **APIs:**
  - `/graphql`: the portal's GraphQL API, for decisions, explanations, policies and dry runs,
    enforcement, the directory, challenges, containments, security keys, investigations, and Awzy
    Bot;
  - `/mcp`: an MCP endpoint for your own agents, with read-only tools and policy drafts.

  Every operation is checked with the PDP as `view`, `audit`, or `administer` on a portal area. Sign-in is
  through your IdP.
- **Stores:** nothing of its own.
- **Reads:** decisions from ClickHouse. Everything else comes from the services above.
- **Writes:** each MCP tool call to awzy-intake, as an event, when awzy-intake is on.

### awzy-reporter

The install's heartbeat to awzy.

- **Reads:** service health from Kubernetes, ClickHouse disk use, Kafka consumer offsets, and
  whether the release synced: the one Argo CD Application that installs it, which the chart lets
  it read and nothing else of Argo CD.
- **Sends:** them, as fixed-schema numbers, to awzy's corp portal every minute. It's the only
  thing that leaves your install for awzy.
- **API:** none beyond health and metrics.

## Jobs

- **`awzy-clickhouse-migrate`** runs once per release and applies ClickHouse's schema before the
  services roll out.
- **`awzy-synthetic`** runs only on demo installs. It provisions Larkspur Capital, the synthetic
  company the demo and tests use.

## On awzy's side

These run in awzy's own account and never receive data about your people, resources, or
decisions:

- **The corp portal** takes in heartbeats at `POST /v1/heartbeat` and keeps `installs`,
  `install_versions`, and `heartbeat_samples` in its own Postgres.
- **The release registry** serves signed images and the Helm chart to your install at `/v2/`
  (the OCI distribution API), with your install's own pull token.

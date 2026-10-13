---
title: Troubleshooting
description: What each of awzy's services and data stores does when it fails, which alert says so, and what your team does about it.
---

The client portal's **Status** page shows whether each of awzy's services is ready, and links
each one to its section here. A service the page can't reach is usually restarting; one that
answers "not ready" is up but waiting on something it needs before it can serve.

Each section says what stops working when that piece fails, which [alerts](/guides/monitoring/)
fire, and what to do. The ones on the decision path come first: while they're healthy, every
check is decided, whatever else is down. To undo an upgrade, see
[rolling back](/guides/upgrading/#rolling-back).

The commands below assume the chart's default namespaces: `awzy-system` for the services,
`awzy-data` for ClickHouse, and `awzy-portal` for the portal.

## Start here

```sh
kubectl -n awzy-ops get application awzy       # Synced and Healthy? A failed sync shows why
kubectl -n awzy-system get pods
kubectl -n awzy-system describe pod <pod>      # why it's restarting, or why it isn't scheduled
kubectl -n awzy-system logs <pod> --previous   # what the last crash said
```

- **A service that crashes at start** most often can't reach Postgres, or finds a schema older than
  it needs. Each release migrates the databases in a Job before the services roll, named
  `<service>-migrate`. Check that it ran:
  `kubectl -n awzy-system get jobs` and `kubectl -n awzy-system logs job/<service>-migrate`.
- **One service not ready usually holds up another.** A starting PDP needs awzy-policy,
  awzy-context, and awzy-challenge, or a running PDP to copy from, so fix those first.
- **Did it start with an upgrade?** If the trouble began when a new release rolled out, rolling
  back is usually faster than finding the cause; see
  [rolling back](/guides/upgrading/#rolling-back).

## awzy-pdp

Decides every check. It holds your policy, the directory, and the containment list in memory, so a
running PDP keeps deciding while the services it learns from are briefly away.

**When it fails:** enforcement points get no answer and follow their outage behavior. awzy-gateway
replays a recent allow for tools in tiers you set to fail open, and denies the rest; your own apps
do what their client library is configured to do.

**Alerts:** `AwzyServiceDown` and `AwzyServiceNotReady` (job `awzy-pdp`), `AwzyPdpCheckErrors`,
`AwzyPdpCheckLatencyHigh`, `AwzyGatewayCantReachPdp`, and the freshness alerts
(`AwzyPdp…Stale`, `AwzyPdpServingStaleResources`).

**What to do:**

- **Not ready:** it hasn't yet loaded the published policy, the directory, and the containment
  list. It takes each from its source (awzy-policy, awzy-context, awzy-challenge), or, when a
  source doesn't answer within 5 seconds, from a running PDP, still verifying the policy's
  signature. So a PDP stays not ready only when a source is down and no other PDP is running. If
  the sources are ready, look in its log for errors reaching them, or for a policy bundle it
  refused because its signature didn't verify.
- **Slow:** compare Cedar time with the total on the dashboard. If Cedar time is low, the PDP is
  waiting on CPU or on a stale feed. The PDP autoscales on CPU up to
  `pdp.autoscaling.maxReplicas` (4 by default); raise that, or `pdp.resources`, in your values, or
  find the caller sending the extra checks.
- **Deciding on stale input:** the freshness alerts name what's stale. Fix the source they name
  (below); the PDP catches up on its own once it's back.

## awzy-gateway

The MCP proxy your agents call. It checks each tool call with the PDP before forwarding it.

**When it fails:** agents' tool calls through it fail; nothing reaches the MCP servers behind it.
When the gateway is up but the PDP isn't, calls follow the outage tiers you set in the client
portal, never replaying an allow for someone contained before the outage.

**Alerts:** `AwzyServiceDown` (job `awzy-gateway`), `AwzyGatewayCantReachPdp`, `AwzyServerErrors`.

**What to do:** check its pods and log. For `AwzyGatewayCantReachPdp`, fix the PDP first, then
the network between the two (a NetworkPolicy, or a PDP Service with no ready endpoints:
`kubectl -n awzy-system get endpoints awzy-pdp`).

## awzy-policy

Keeps, signs, and serves policy bundles, enforcement modes, and the outage fallback.

**When it fails:** the portal can't show or publish policies, nobody can switch enforcement modes,
and a PDP that starts takes policy from a running one. Running PDPs keep deciding on the policy they
have.

**Alerts:** `AwzyServiceDown` or `AwzyServiceNotReady` (job `awzy-policy`), then
`AwzyPdpPolicySyncStale` from every PDP.

**What to do:** it's ready once it has started, so if it's unreachable it's restarting. Its log
says whether it couldn't reach Postgres, found an old schema, or couldn't use the signing key in
your cloud's key service (its workload identity must be allowed to sign with it).

## awzy-context

Keeps what awzy knows about your organization: people and groups from your IdP, service accounts,
resources and work items from connectors, and risk scores.

**When it fails:** SCIM provisioning from your IdP fails until it's back (your IdP retries), the
portal's Directory page is empty, and a PDP that starts takes the directory from a running one. Running PDPs
keep their directory, and decide on resources they've cached, but a resource they haven't seen
fails its check.

**Alerts:** `AwzyServiceDown` or `AwzyServiceNotReady` (job `awzy-context`), then
`AwzyPdpPrincipalsSyncStale`, `AwzyPdpResourceFeedStale`, and `AwzyPdpServingStaleResources`.

**What to do:** it restarts for the same reasons as awzy-policy: check its log for Postgres or the
schema. While the directory is stale, a person your IdP removed may still be allowed; if that
matters now, contain them in the client portal, which reaches every PDP within seconds.

## awzy-challenge

Keeps challenges, containments, and security keys, and pushes the containment list to every PDP.

**When it fails:** nobody can answer a challenge or contain someone, and the portal's Challenges
page can't load. Once its feed has been quiet for 2 seconds, the PDPs ask awzy-challenge directly;
with no answer, they decide on the last containment list they had. That list stands in for up to 5
minutes after the feed was last heard (`pdp.containmentsMaxStaleSeconds`), which covers a database
failover. Past that, checks fail and enforcement points follow their outage behavior: a PDP never
decides someone isn't contained on a list it can't trust. A challenge verdict stays a challenge: a request is never
allowed for want of an answer.

**Alerts:** `AwzyServiceDown` or `AwzyServiceNotReady` (job `awzy-challenge`),
`AwzyPdpContainmentFeedStale`, then, after 5 minutes, `AwzyPdpCheckErrors`.

**What to do:** it's not ready until it has connected to Postgres and loaded the containment list,
so a PDP never starts with an empty one by mistake. Check its log for Postgres errors. If
awzy-challenge is ready and a PDP still reports the feed stale, check the network between them.

## Postgres

Your managed Postgres holds three databases: awzy-policy's, awzy-context's, and awzy-challenge's.

**When it fails:** all three of those services lose their store at once. Running PDPs keep
deciding on what they hold in memory (policy, directory, cached resources, the last containment
list, for up to 5 minutes), but nothing can change: no publishes, no SCIM changes, no new
challenges or containments. After a failover, the services reconnect on their own. If Postgres is
still down after 5 minutes, checks start failing on the stale containment list, and enforcement
points follow their outage behavior.

**Alerts:** `AwzyServiceNotReady` or `AwzyServerErrors` from awzy-policy, awzy-context, and
awzy-challenge together, then the PDP freshness alerts. Your cloud's own alarms for the database
(CloudWatch for Aurora, Cloud Monitoring for Cloud SQL) say why.

**What to do:**

1. Check the database in your cloud's console: a failover in progress, storage full, or too many
   connections.
2. Once it answers, the services reconnect. If a pod stays unready, restart it:
   `kubectl -n awzy-system rollout restart deploy/<service>`.
3. If the data itself is lost, restore it with your cloud's point-in-time restore, and talk to awzy
   support before pointing the install at a new instance. Each service's migrations run again on
   start and change nothing that's already there.

## Kafka

Carries decisions from the PDPs, and events from awzy-intake, to ClickHouse. Nothing on the
decision path waits for it.

**When it fails:** decisions are still made. Each PDP holds decision records in memory while Kafka
is away (about 20,000 per pod) and sends them when it's back; past that, records are dropped and
counted, so the audit trail has a gap. A PDP that starts while Kafka is away serves checks too,
holding its records the same way, so pods can still be replaced and added. awzy-intake answers senders with an error, so they retry
later and nothing they sent is lost. The portal's Decisions page and awzy-reasoner fall behind.

**Alerts:** your cloud's alarms for the cluster (MSK on AWS), and Kafka lag on the dashboard.
`AwzyServerErrors` from awzy-intake, and `AwzyPdpDroppingDecisions` once a PDP's buffer is full.

**What to do:** check the cluster in your cloud's console: brokers down, storage full, or
connections refused. Once it's back, ClickHouse catches up from the topics on its own. Kafka keeps
decisions and events for at least 7 days, so a longer ClickHouse outage loses nothing either.

## ClickHouse

The portal reads decisions and activity from ClickHouse, and awzy-reasoner scores risk from it.
"Not configured" means your install runs without it. "Unreachable" means the portal couldn't run a
query:

```sh
kubectl -n awzy-data get pods
kubectl -n awzy-data logs <clickhouse-pod>
```

**When it fails:** decision search and activity in the portal show as unavailable, and
awzy-reasoner's scoring step fails, so PDPs decide on the last risk scores. Decisions keep flowing,
and nothing is lost: they wait in Kafka and are written when it's back.

**Alerts:** `AwzyReasonerStepFailing` (step `scoring`), and ClickHouse's disk use on the dashboard.

**What to do:** most often its disk is full or a pod can't be scheduled. If its data is lost, restore
the last backup and refill the rest from Kafka (see [data retention](/guides/data-retention/)).

## awzy-intake

Takes in your app and cloud audit logs.

**When it fails:** senders get errors and retry; events arrive late, and risk scores that use them
lag. Decisions aren't affected.

**Alerts:** `AwzyServiceDown` (job `awzy-intake`), `AwzyServerErrors`.

**What to do:** check its log. A 5xx usually means Kafka is unreachable (above); a 401 to a sender
means its token doesn't match the one the install was given.

## awzy-reasoner

Scores risk every 15 minutes, contains people automatically, and investigates the ones it flags.

**When it fails:** the PDPs decide on the last risk scores, nobody is contained automatically, and
no investigations run. Decisions aren't otherwise affected, and a missed run is made up by the
next.

**Alerts:** `AwzyReasonerStepFailing`, which names the step; see
[its section](/guides/monitoring/#awzyreasonerstepfailing-ticket) for what each step depends on.

## awzy-connectors

Brings resources and work items in from your Google Drive and Jira.

**When it fails:** new and changed resources and work items stop arriving. The PDP decides on the
last ones it knows, and a resource it has never heard of gets your default tier and is marked
unclassified, which policies can challenge.

**What to do:** its log says which source failed. Most often it's the source's credential: a
Google service account whose domain-wide delegation was removed, or a Jira API token that
expired (see [connectors](/guides/connectors/)).

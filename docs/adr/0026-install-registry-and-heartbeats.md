# 0026. The corp portal knows installs from a registry and a fixed-schema heartbeat

- Status: Accepted; amended by [0041](0041-fleet-health-history-in-the-corp-portal-postgres.md) and
  [0042](0042-reporter-reads-clickhouse-and-kafka-offsets.md)
- Date: 2026-10-09

## Context

awzy staff need one place to see every deployed install and the units inside it: which
services run, at which versions, whether they are healthy, and how a release is rolling out.
0009 limits what that place can know. awzy has no access to an install, and installs push only
health data, in a fixed schema the client can inspect and turn off.

## Decision

- **Registry first.** Staff register an install in the corp portal before it reports: client,
  environment, cloud, region, release channel (`stable`, `fast`, or a pinned version). Registering
  creates an install ID (`install_<uuid>`) and a reporting token, shown once. Only a SHA-256 hash
  of the token is stored. The client puts the ID and token in their install's values. Heartbeats
  from unregistered installs or with an unknown token are rejected, so the corp portal only holds
  installs awzy has agreed to serve.
- **The client's name never leaves the install.** It comes from the registry; the heartbeat carries
  only the install ID.
- **One heartbeat, every 60 seconds,** from `awzy-reporter` in the install to `POST /v1/heartbeat`
  on the corp portal, with the token as a bearer token. Schema version 1 (`libs/heartbeat`):
  - the chart version;
  - per service, from a closed list of awzy services and ClickHouse: version, image digest,
    desired and ready replicas, restarts over 24 hours, and its `/readyz`;
  - aggregate metrics, all optional: decisions per second, PDP p99, error rate, Kafka lag,
    ClickHouse disk use.
- **Nothing free-form.** Unknown fields and unknown services are rejected, and every string must
  match a narrow pattern (versions, `sha256:` digests). A hostname, user name, label, or policy
  text can't ride along in a valid heartbeat.
- **The client can read and stop it.** The reporter logs every heartbeat it sends, can print one
  without sending it, and is turned off with one value. An install that stops reporting stays in
  the registry, shown as "not reporting since".
- **Health** is derived in the corp portal: not reporting (no heartbeat for 5 minutes), degraded
  (a service isn't ready or has fewer ready replicas than desired), or ok.
- awzy's own staging install registers and reports like any client install.

## Consequences

- The corp portal keeps each install's latest heartbeat and its version history (when each chart
  version was first seen) in its Postgres. Health time series move to ClickHouse when the fleet
  needs charts.
- A new field means a new schema version, accepted by the corp portal before any reporter sends
  it. Clients can review each version before upgrading.
- The reporting token is a credential only for sending heartbeats; it reads nothing. Rotation is
  re-registering for now.
- The reporter needs read access to the awzy namespaces' Deployments and Pods in Kubernetes, and
  nothing else. Locally it probes the services' `/readyz` directly.

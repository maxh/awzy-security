---
title: Platform team
description: Installing, upgrading, and running awzy in your cloud account.
---

Your platform team runs the install: awzy's services, its data stores, and the cluster they run
in, all in your own cloud account. awzy has no access to any of it.

## Installing

1. **Register the install with awzy.** You get an install ID, a registry pull token, and a
   reporting token for the heartbeat.
2. **Apply the cloud layer** (`aws/install`): the network, the Kubernetes cluster, managed Postgres
   and Kafka, the cache, and the encryption key. Choose a size profile.
3. **Apply the cluster layer** (`bootstrap`): Argo CD, Kyverno, and the operators, then the awzy
   chart, pulled from awzy's registry with your token.
4. **Point DNS** at the gateway for the client portal's hostname, and give your IdP the SCIM URL and
   token so it provisions people and groups.

See [Installing awzy](/guides/install/) for each step, and
[Turning on optional features](/guides/optional-features/) for Awzy Bot, investigations, and Slack.

## Upgrading

Your install pulls new releases itself, from the channel you follow or the exact version you pin.
Kyverno admits only images awzy signed. Upgrades roll services without dropping decisions. See
[Upgrading awzy](/guides/upgrading/).

## Running it

- **Health:** every service serves `/healthz`, `/readyz`, and `/metrics`. Point your Prometheus at
  them. A Grafana dashboard and Prometheus alert rules are in the repository's
  [`deploy/observability`](https://github.com/maxh/awzy-security/tree/main/deploy/observability);
  the install doesn't deploy them for you yet.
- **The heartbeat** is the only thing that leaves your install for awzy: service health and a few
  numbers, never data about your people, resources, or decisions. See the
  [security model](/reference/security-model/).
- **Secrets** live in your cloud's secret store and Kubernetes Secrets. Rotate the SCIM token by
  replacing it in Terraform and giving your IdP the new one.
- **Where data lives:** see [Services](/reference/services/) for each service's database, topics,
  and APIs.

## When things go wrong

- **A service isn't ready:** the client portal's Status page says which, and
  [Troubleshooting](/guides/troubleshooting/) says what to check for each.
- **The PDP is unreachable:** awzy-gateway denies calls, except recent allows for low-sensitivity
  tools your admins let fail open on the client portal's Enforcement page, and never for anyone
  contained.
- **A bad policy locks admins out:** an enrolled admin rolls back to an earlier bundle with a
  break-glass key, which works without the portal or your IdP. See
  [Writing policies](/guides/writing-policies/).
- **awzy's help:** awzy never has access to your install, so it helps from what you share with it.
  Read-only support sessions that you approve, and that expire on their own, are planned.

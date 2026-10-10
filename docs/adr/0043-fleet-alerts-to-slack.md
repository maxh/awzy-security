# 0043. Fleet alerts go to Slack from the corp portal

- Status: Accepted
- Date: 2026-10-09
- Builds on: [0026](0026-install-registry-and-heartbeats.md)

## Context

The corp portal derives each install's health from its heartbeats (0026): ok, degraded, or not
reporting. Nobody hears about a change unless they happen to be looking at the Fleet page. The
platform plan makes health alerts awzy's job.

## Decision

- **The corp portal backend checks every install once a minute** and compares its health with
  the last health it alerted on, stored on the install (`alerted_health`).
- **Changes hold before they're sent.** Degraded, and recovery, must last 3 minutes, so a single
  heartbeat with a pod restarting doesn't page anyone. Not reporting is sent at once: it already
  means 5 minutes without a heartbeat. A change that undoes itself while held is dropped. An
  install's first healthy heartbeat isn't an alert.
- **One sender.** The task that wins a compare-and-set on `alerted_health` sends the alert, so
  two corp portal tasks during a deploy don't both send it. If Slack refuses it, the claim is
  undone and the next check tries again.
- **Slack incoming webhook.** The URL lives in Secrets Manager (`awzy-corp-portal/alerts-slack-webhook`),
  set by hand, never in Terraform state. Until it's a `https://hooks.slack.com/` URL, alerts are
  only logged.
- **What a message says:** the install's client and environment (from awzy's own registry), the
  new health, which services aren't fully ready (names and replica counts from the heartbeat),
  how long since the last heartbeat, and a link to the install in the corp portal. Nothing else
  from inside an install (0009).

Rejected for now: PagerDuty (paging with escalation is more than a small team needs yet) and email
through SES (domain verification and leaving the SES sandbox, for less reach than Slack).

## Consequences

- Slack holds awzy's client list as install names in alert messages. That's awzy's business data,
  not a client's.
- One webhook, one channel, no acknowledgement or escalation. Moving to PagerDuty later means a
  second `Notifier`, not a new design.
- An outage of the corp portal itself sends nothing; watching the portal is a separate job.

---
title: Sending events to awzy-intake
description: Getting your apps' access logs and CloudTrail into awzy, so they count toward risk.
---

awzy-reasoner scores each person and service account from what the PDPs decided and from events:
what your apps and your cloud's audit logs say they did. awzy-intake takes those events in,
normalizes them, and hands them to ClickHouse through Kafka. Turn it on with the `intake` variable
in the cluster layer's Terraform (see [Installing awzy](/guides/install/), step 5).

## Where to send them, and how

- **URL:** `http://awzy-intake.awzy-system.svc:8080` inside the cluster, or
  `https://<intake hostname>` when you set one for sources outside it.
- **Token:** every request carries `Authorization: Bearer <token>`. Read it once with
  `tofu output -json intake`.
- **Batches:** up to 1,000 events a request. A batch is all or nothing: one bad event rejects the
  request with `400`, naming it, so a shipper never half-delivers. `202` means Kafka has them;
  `503` means retry.
- **Times** are RFC 3339 and may carry any offset; awzy stores them in UTC. An event more than five
  minutes in the future is refused as a bad clock.

## Your apps' events

`POST /v1/events` with awzy's own schema:

```json
{
  "events": [
    {
      "at": "2026-10-09T10:30:00-04:00",
      "system": "jira",
      "principal": { "type": "Human", "id": "bob@example.com" },
      "on_behalf_of": null,
      "action": "export",
      "resource": { "type": "Ticket", "id": "ENG-42" },
      "outcome": "success",
      "source_ip": "203.0.113.7",
      "attributes": { "project": "ENG" }
    }
  ]
}
```

- `principal` is a `Human`, by the user name your IdP provisions over SCIM, or a registered
  `ServiceAccount`, with `on_behalf_of` naming the person it acted for.
- `outcome` is `success` or `failure`, or left out when the app doesn't know. Failures count
  against a principal like denials do.
- `action` is your app's own word, with four that awzy treats as moving data out, for automated
  containments: `download`, `export`, `send_external`, and `share_external`.
- `attributes` holds anything else worth keeping. Don't put document contents there.

## AWS CloudTrail

`POST /v1/events/aws-cloudtrail` with a CloudTrail log file's JSON, `{"Records": [...]}`, as
CloudTrail writes it to S3 (decompressed). A small function triggered by new log objects in the
trail's bucket can read each one and send it.

- Each record becomes an event: its source, its name as the action, the first resource it lists,
  and `failure` when it carries an error code.
- A session of an IAM Identity Center role is recorded as the person it names, so their AWS
  activity counts toward their scores. List other roles whose session names are people in the
  chart's `intake.humanRoles`. Every other caller stays as CloudTrail named it, and the original
  ARN is kept with the event either way.

## What awzy records itself

awzy-gateway records each tool call it forwards, and the client portal's MCP endpoint each tool
call it serves, as events of their own when awzy-intake is on, so an agent's work through awzy
counts toward its scores too.

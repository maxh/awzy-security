---
title: Client libraries
description: "Checking calls with awzy from your own code: what every client library does, and which languages have one."
---

A client library makes your own code an enforcement point: before your app or agent takes an
action, it asks awzy, and applies the answer. awzy-gateway does this for MCP tool calls without
any code; a client library does it for everything else your code does.

| Language          | Package           |                                             |
| ----------------- | ----------------- | ------------------------------------------- |
| TypeScript (Node) | `@awzy/authz-sdk` | [TypeScript](/client-libraries/typescript/) |

## What every library does

Every client library behaves the same way, and each is held to the same set of scenarios, so a
check gives the same answer whichever language asks it.

- **Validates the caller's token** (signature, issuer, audience, expiry), and takes who is acting
  from it, never from your code. An agent acting for a person is checked as both.
- **Sends IDs, not attributes.** awzy decides on its own data about people and resources; only
  your app's own entity types carry the attributes your app sends.
- **Applies the mode.** In monitor mode every call is allowed, and the result says what enforcing
  would have done.
- **Surfaces challenges** as a result you can wait on: awzy asks the person, and the wait ends
  when they answer, so you retry the same call.
- **Falls back as awzy-gateway does** when awzy can't answer within the call's budget: allowed
  while the install monitors, refused when it enforces, unless you set the action to fail open,
  the resource's tier may, and awzy allowed the same call recently. `confidential` and
  `crown_jewels` never fail open.
- **Records each decision's ID** for your logs, and can record what it allowed as activity, so
  awzy's risk scores see what agents do through your app.
- **Plans lists and searches**: which records a principal may see, as a filter for your own query.

## Without a library

From a language without one yet, call the [decision API](/reference/decision-api/) directly, over
gRPC from `awzy.authz.v1` or as JSON over HTTP. Your code then has to do each of the things
above itself; a call it can't make, or an answer it can't read, should be refused.

---
title: Principles
description: The rules awzy follows, and what it will never do.
---

Each of these holds everywhere in awzy. The linked pages explain how.

## How awzy decides

- **Decisions are fast because the thinking happens first.** Anything slow, such as classifying a
  document, scoring risk, or running an investigation, happens ahead of time and is stored with
  the person or resource. A check only reads what's stored and evaluates policy, so it never waits
  on a model and takes milliseconds. See [how awzy decides](/concepts/how-awzy-decides/).
- **One decision, whatever the path.** A tool call through the MCP gateway and a request your own
  app sends to the [decision API](/reference/decision-api/) are decided by the same PDP and the same
  policies.
- **An agent never gets more than its person.** An agent acting for someone is checked as itself
  and as them, and the stricter verdict wins. See
  [people, agents, and delegation](/concepts/principals-and-delegation/).
- **Watch before you enforce.** A new install records what it would deny and challenge without
  applying it, and so do automated containments. You switch to enforce after reviewing them, with
  your security key. See [monitor, then enforce](/concepts/monitor-and-enforce/).
- **Ask before refusing.** Where a flat deny would stop someone's work, a challenge asks for a
  justification, a security key, or someone's approval instead. See
  [challenges and containment](/concepts/challenges-and-containment/).
- **Fail safe.** A PDP that hasn't loaded your policy refuses to decide rather than guess. When
  the PDP can't be reached, the gateway denies, except for tools you've chosen in low-sensitivity
  tiers, and never for someone contained before the outage. See
  [the MCP gateway](/guides/mcp-gateway/#when-the-pdp-cant-be-reached).

## Where AI fits

- **Models can raise risk, never lower it.** Risk scores and sensitivity come from rules first. An
  investigation can raise a score but never take it below the rules' score, and a sensitivity
  label is never lowered. See [risk](/concepts/risk/) and [investigations](/concepts/investigations/).
- **Agents propose; people decide.** Agents can draft a policy or ask for a containment to be
  lifted. A person publishes and lifts, and only a person can pass a security-key challenge.
- **Models run in your cloud.** Awzy Bot and investigations use your own cloud's model service,
  and are off until you turn them on. See [optional features](/guides/optional-features/).

## Your data and your install

- **Your data stays in your account.** awzy has no access to your install, even in an emergency,
  and only a fixed-schema heartbeat leaves it, which you can turn off. See the
  [security model](/reference/security-model/).
- **Whatever changes enforcement is signed.** Policy, the switch between monitor and enforce, and
  every image awzy ships. Your install applies only what verifies.
- **There's always a way back.** Break-glass rollback works with keys your admins enroll at
  install, even if the IdP, the PDP, or the client portal is down. See
  [break-glass](/guides/break-glass/).
- **awzy follows its own rules.** The client portal and its MCP endpoint are authorized by the PDP
  like any other app, and what agents do there is recorded like anywhere else.

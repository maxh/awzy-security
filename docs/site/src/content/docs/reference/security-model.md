---
title: Security model
description: What awzy can and can't see, and how releases reach your install.
---

Who these defend against, attacker by attacker, and how each defense is checked, is in
[Threat model](/reference/threat-model/).

- **Single-tenant, in your account.** Each client runs its own install in its own cloud
  account. There is no shared awzy service holding client data.
- **No vendor access.** awzy has no credentials for your install, even in an emergency. Repairs
  are run by your team. When awzy support needs to look inside, they ask, and an admin of yours
  approves a support session in the client portal: read-only, limited to the areas you choose,
  ending on its own or when you end it, and recording everything it opens.
- **Pull-based, signed releases.** Your install pulls releases from awzy's registry with its own
  revocable token. Every image is signed with awzy's release key and carries its SBOM, signed
  with the same key, and the cluster refuses any image in awzy's namespaces that isn't from that
  registry with both, third-party images included. Each release's chart carries an SBOM of every
  Rust crate and npm package it's built from.
- **A heartbeat, and nothing else, to awzy.** The only thing that leaves your install for awzy is
  a heartbeat to its corp portal: versions, readiness, whether the release synced, and a few
  aggregate numbers, in a fixed schema with no free-form fields. You can turn it off. A diagnostics bundle from the client
  portal reaches awzy only if an admin downloads it and sends it, and holds no data about your
  people, resources, decisions, or policies. How the heartbeat compares with your own monitoring
  is in [Monitoring and alerts](/guides/monitoring/#what-awzy-sees-and-what-you-see).
- **Your own services, only when you turn them on.** Awzy Bot and investigations send prompts,
  including decisions and document text, to the model service in your own cloud account. Notifications
  send short messages naming the request to your own Slack workspace or through your own mail
  relay. Connectors
  read your Google Drive and Jira. Each is off until you turn it on; see
  [optional features](/guides/optional-features/).
- **Services authenticate to each other.** Inside the install, every call between awzy's
  services carries a short-lived Kubernetes service-account token, checked by the receiver.
- **Policy is signed.** awzy-policy signs every bundle it publishes, every switch between monitor
  and enforce, and every change to which tools fail open during an outage, with a key in your
  cloud's key service (KMS on AWS). The private key never leaves it, and only awzy-policy's own
  workload identity may sign with it.
  PDPs and awzy-gateway apply only what verifies, and never an older version than the one they've
  applied, so someone who can write to awzy-policy's database can't change what's enforced. Every publish, rollback, and mode change is
  recorded with who did it.
- **awzy knows an agent only by its token.** An agent with its own identity, through a token
  exchange, is checked as itself and as its person, and can't answer their challenges. An agent
  using a person's own token is that person to awzy; only a security key proves the person
  themselves was there. The client portal's API, its MCP endpoint, and awzy-gateway each take
  tokens for their own audience, so a token handed to an agent for one doesn't open the others.
  See
  [People, agents, and delegation](/concepts/principals-and-delegation/).

## Your apps, their agents, and client libraries

When your own apps check with awzy and mint their own agents, these are what you're trusting,
and what limits each.

- **A trusted app vouches only for its own agents.** Trusting an app as a token issuer means
  trusting its signing key, so treat that key like a credential. awzy limits what it can sign:
  tokens only for service accounts whose ID starts with the app's name and a dot, on their own or
  acting for a person, and never for a person on their own or for any other service account. No two apps share an
  issuer, a token with your IdP's issuer is always checked as your IdP's, and an app's keys come
  over HTTPS only. Removing the app
  stops its tokens within about ten seconds; agents it registered stay registered until removed.
- **An app manages only the agents under its name.** Its registration API takes a token the app
  signs for itself, and registers or retires only IDs under its prefix. Every agent still needs
  an owner your IdP provisioned, and every change is recorded with the app as who made it. A
  token minted for an agent can't register agents.
- **A client library is an enforcement point in your app.** It validates callers' tokens itself
  and applies awzy's answer, so awzy decides what it's asked, and your app has to ask. Calls to
  awzy-pdp need a service token from a workload the install trusts. When awzy can't be reached, a
  library refuses in enforce mode unless you set that action to fail open on a tier your client
  portal allows, and awzy allowed the same call recently; `confidential` and `crown_jewels` never
  fail open.
- **Waiting on a challenge reveals nothing else.** An agent can wait only on a challenge its own
  call opened, acting for the same person; anyone else's isn't found. The answer stays in awzy, so
  waiting hands the agent nothing it could replay, and only the person, never the agent, can
  answer.
- **A query plan shows the shape of a principal's policy.** A plan's filter carries the
  conditions policy applies to that principal, such as which tenants or folders it may see and the
  groups it's in. It goes only to the enforcement point that asked, which should apply it and not
  pass it on to people or agents. A filter over awzy's own resource labels is only as current as
  your app's copy of them, and what a filter can't decide from the row is left for row-by-row
  checks rather than guessed. Plans are recorded, a contained principal's next plan or next page is
  refused, and your app reports what each one returned, so a bulk read through plans still counts toward risk.

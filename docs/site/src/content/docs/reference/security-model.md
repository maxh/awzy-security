---
title: Security model
description: What awzy can and can't see, and how releases reach your install.
---

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
  including decisions and document text, to the model service in your own cloud account. Slack
  notifications send short messages naming the request to your own Slack workspace. Connectors
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

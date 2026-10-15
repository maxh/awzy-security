---
title: Threat model
description: Who awzy defends against, what each of them can try, what stops it, and how that's checked.
---

awzy decides who and what may act on your data, so it's a target in its own right. This page goes
through who might attack an install, what each can try, and what stops them. It ends with how
those defenses are checked. [Security model](/reference/security-model/) describes the mechanisms
themselves.

## Out of scope

Someone with admin rights to your cluster or your cloud account can change anything in an
install, including awzy's code and keys. awzy doesn't defend against them. Protect those
accounts as you would any other system's.

## An outsider with no credentials

- **Every API needs a token** from your IdP, an app you trust as an issuer, or, between awzy's
  own services, a Kubernetes service-account token the receiver checks. Each surface takes
  tokens for its own audience only.
- **The MCP gateway fails closed on what it can't read.** A request body that its parser refuses
  is rejected, not forwarded, because a more lenient server behind it might read the same body as
  a tool call no one checked. Batched tool calls are refused too.
- **IDs have one spelling.** awzy's IDs parse only in the form awzy writes them, so a check that
  compares two IDs can't be sidestepped by writing one differently.

## An AI agent, compromised or misbehaving

An agent may have its own identity, through a token exchange, or use a person's own token.

- **An agent with its own identity is checked as itself and as its person.** The stricter answer
  wins, so it never gets more than both could.
- **It can't answer its person's challenges**, and it can't approve a challenge for the person it
  answers to. Approving needs a human whenever the data is `crown_jewels`.
- **Acting on its own authority, an AI agent never reaches `crown_jewels` data,** whatever a
  challenge says.
- **An agent using a person's token is that person to awzy.** When awzy-reasoner scores a
  person's requests as software's, sensitive reads, writes, and tool calls need a security key,
  which an agent can't touch.
- **Tool calls through the MCP gateway are checked per tool.** During a PDP outage, a tool keeps
  its own mode: a tool left in monitor doesn't open the others. A replayed allow is the tool's
  latest decision, and only for tiers you let fail open. `confidential` and `crown_jewels` never
  fail open.

## Someone with a person's session

- **Switching between monitor and enforce, and configuring connectors, need a security key every
  time**, so a stolen session or an agent with someone's credentials can't do either.
- **A person deactivated in your IdP gets nothing**, whatever their token says.
- **High risk turns an allow into a security-key challenge**, and someone whose risk is elevated
  needs their key for high-risk tool calls.

## An insider who can write to awzy's databases

They can't sign: the signing key stays in your cloud's key service, and only awzy-policy's
workload identity can use it. They can replay old rows, delete rows, and forge new ones.

- **PDPs and the gateway apply only what awzy-policy signed:** bundles, enforcement modes, and
  which tools fail open during an outage. A forged row is refused.
- **Nothing goes backwards.** A PDP never applies an older bundle or mode than it has, and never
  drops a tool's own mode because its rows were deleted.
- **A starting PDP takes its running peers' policy first,** so deleting rows can't start it in an
  older or weaker policy than they enforce. The remaining gap: a PDP that starts while no peer is
  running has only awzy-policy's answer to go on.
- **Every publish, rollback, and mode change is recorded with who made it.**

## A source system or connector that changes data

Connectors read your systems, such as Google Drive and Jira, and tell awzy what each resource is:
its tier, owner, and sharing.

- **Configuring a connector or its credential needs a security key.**
- **Changes reach every PDP within about a minute.** The change log can't skip a write that
  commits late, and a fetch that a change overtook isn't cached.
- **A resource awzy hasn't classified gets the default tier,** and policies can challenge
  unclassified resources.

## A contained principal

- **Containment reaches every PDP over a feed, and a PDP never goes back to an older list,**
  so a containment it has enforced isn't forgotten. A replica that can't read its database stops
  vouching for its list within seconds. The PDPs then ask directly, and stop deciding from a list
  older than their configured limit.

## awzy, the vendor

- **awzy has no credentials for your install.** Support sessions are requested by awzy staff
  and approved by your admins. They're view-only, limited to the areas an admin approved, and end
  on their own.
- **Releases are pulled and signed.** Your cluster refuses images in awzy's namespaces that aren't
  signed by awzy's release key and don't carry their SBOM.
- **Only a heartbeat leaves the install**, in a fixed schema with no free-form fields, and you can
  turn it off.

## How these are checked

- **Policy proofs.** The base policies are proved, over every request the schema allows, to keep
  the promises on this page that policy makes. For example: a deactivated person is never
  allowed, an AI agent on its own never reaches `crown_jewels`, only the security team
  administers, agents never switch enforcement, and support sessions only view their approved
  areas. No base `forbid` can error, because a policy that errors is skipped and a skipped
  `forbid` would fail open. A change that breaks a proof fails awzy's build, with a request that
  shows it.
- **Protocol models.** How containments, signed policy, challenges, resource changes, and the
  gateway's outage fallback move between services is modeled and checked against every
  interleaving of messages, failures, and restarts that the checker tries, on every change and at
  much greater depth nightly. Each bug the models found stays in them behind a switch, and the
  check fails if a model stops catching it.
- **Fuzzing.** The parsers that take input an attacker controls, such as IDs, signatures, and
  the actors in audit records, are fuzzed on every change and nightly.
- **Dependencies.** Every Rust crate, and every npm package the portals ship, is checked against
  the public advisory databases daily. Rust crates' licenses and sources are checked on every
  change.
- **Reporting a vulnerability:** see `SECURITY.md` in the repository.

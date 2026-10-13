---
title: Glossary
description: The words awzy uses, in one place, with where to read more.
---

**Accessor.** See *principal*. It's the Beyond Zero paper's word for one.

**Acting for someone.** A service account working for a person, such as an AI agent prompted by
them. Both are checked, and the stricter verdict wins, so an agent never gets more than both of
them could. See [People, agents, and delegation](/concepts/principals-and-delegation/).

**Action.** What a principal wants to do: `read` and `write` on a resource, `call` on an MCP tool,
`view`, `audit`, and `administer` on a portal area, `switch_enforcement` to switch between monitor and
enforce, and `approve` on someone else's challenged request.

**Actor.** Who did something that awzy records, such as publishing a bundle or lifting a
containment: an entity, and the person it acted for, if any. See the
[domain model](/concepts/domain-model/#who-did-something).

**Approval.** A challenge that someone else answers: whoever your policy allows to approve the
resource, such as its owner or the security team, or an agent you name. See
[Challenges and containment](/concepts/challenges-and-containment/).

**Assigned work.** Whether the principal has open work, such as a ticket, an account, or a
statement of work, that links the resource or a folder above it. Policies see it as
`context.assigned`. See [work items](/concepts/domain-model/#work-items).

**Automated containment.** A containment awzy-reasoner makes on its own, when a person's risk is
elevated, a challenge for them was rejected, and they tried to move data out, all in the same
day. It has its own enforcement mode, which starts in monitor: until you switch it, the
containment is only recorded, as "would contain".

**awzy-challenge.** The service that keeps challenges and containments, and sends every PDP the
current list of who is contained.

**awzy-context.** The service that keeps what awzy knows about your organization: people and
groups from your IdP, service accounts, resources and work items from connectors, risk scores,
and investigations.

**awzy-gateway.** An enforcement point that sits in front of your MCP servers and checks every tool
call before it runs.

**awzy-intake.** The service that takes in your apps' and cloud's audit logs as events.

**awzy-pdp.** The policy decision point: the service that decides every check, in a few
milliseconds.

**awzy-policy.** The service that keeps your policy bundles, signs them, and holds each enforcement
mode.

**awzy-reasoner.** The service that scores risk every 15 minutes, contains people when the day's
evidence adds up, and runs investigations.

**Break-glass key.** An admin's key, enrolled at install, that rolls back to an earlier policy
bundle without the client portal or your IdP, for when a bad bundle locks admins out. See
[Writing policies](/guides/writing-policies/).

**Cedar.** The policy language awzy uses. See [cedarpolicy.com](https://www.cedarpolicy.com) and
[Writing policies](/guides/writing-policies/).

**Challenge.** A verdict that means "yes, once you've done this": a justification, a security key,
or an approval. The caller retries once it's passed. See
[Challenges and containment](/concepts/challenges-and-containment/).

**Check.** One question to awzy-pdp: may this principal take this action on this resource, in this
context? See [How awzy decides](/concepts/how-awzy-decides/).

**Client portal.** The web app in your install where your team searches and explains decisions,
writes policy, switches modes, answers challenges, and reviews investigations. It also serves an
MCP endpoint for your own agents.

**Connector.** What brings resources and work items into awzy-context from your systems, such as
Google Drive and Jira.

**Containment.** A stop on everything a principal does, until someone lifts it. A contained
person's agents are stopped too. See
[Challenges and containment](/concepts/challenges-and-containment/).

**Crown jewels.** The highest sensitivity tier: data whose exposure would do the most harm.

**Decision.** awzy-pdp's answer to one check: its verdict, the policies that produced it, and its
mode. Every decision is logged with a QID and can be explained. See
[How awzy decides](/concepts/how-awzy-decides/).

**Elevated risk.** A risk score above 40. On its own it changes nothing, but some policies use it,
such as asking for a security key before a high-risk operation. See [Risk scores](/concepts/risk/).

**Enforce.** The mode in which enforcement points apply verdicts. See
[Monitor, then enforce](/concepts/monitor-and-enforce/).

**Enforcement point.** Software that sends checks to awzy-pdp and applies the verdict: awzy-gateway
for MCP, or your own code through the [decision API](/reference/decision-api/).

**Event.** One thing a principal did, as an app or your cloud's audit log recorded it, taken in by
awzy-intake.

**Explain.** How the client portal shows why a decision came out the way it did: the policies that
decided it, with their source.

**Heartbeat.** The only thing that leaves your install: health numbers sent to awzy. It carries no
data about your people, resources, or decisions. See the
[security model](/reference/security-model/).

**High-risk operation.** A `crown_jewels` MCP tool, such as resetting credentials or exporting the
directory. Someone at elevated risk touches their security key before running one.

**Install.** One copy of awzy, in your own cloud account, that only your organization uses. See
[Installing awzy](/guides/install/).

**Investigation.** A model's look at one principal that awzy-reasoner flagged. It ends in a finding,
and awzy's checks, in code, decide what the finding may do: it can't lower a score, can't
contain anyone the scores don't flag, and can't contain on evidence the principal could have
written. Off until you turn it on.

**Justification.** A challenge the person answers by saying why they need the access.

**MCP.** The Model Context Protocol, which AI agents use to call tools. awzy-gateway checks those
calls, and the client portal serves its own MCP endpoint.

**Monitor.** The mode every install starts in: awzy decides and records every check, but
enforcement points allow everything, so you can see what enforcing would do first. See
[Monitor, then enforce](/concepts/monitor-and-enforce/).

**Notifications.** Messages in your own Slack when something waits for someone: a person who
needs to give a reason or touch their security key, an owner or the security team asked to
approve, a requester whose request was decided, and the security team about containments. They
link into the client portal and never carry document contents or evidence. Off until you add a
Slack app's token.

**Policy.** A Cedar `permit` or `forbid` rule. A `forbid` annotated with `@challenge(...)` asks for
a challenge instead of denying.

**Policy bundle.** A signed, versioned set of your policies and their schema. One bundle is active
at a time. See [Writing policies](/guides/writing-policies/).

**Portal area.** A part of the client portal that policy controls: `decisions`, `policies`,
`enforcement`, `directory`, `challenges`, and `investigations`.

**Principal.** Who a check is about: a `Human` or a `ServiceAccount`, written `principal` in your
policies (`principal is Human`) and in the decision API. See
[People, agents, and delegation](/concepts/principals-and-delegation/).

**QID.** The ID awzy gives its own records: the record's type, an underscore, and a UUID, such as
`decision_0199a8f2-…`. See the [domain model](/reference/api-conventions/#qids).

**Resource.** Something people access: a document, a folder, a repository. Resources nest, and
carry a sensitivity, an owner, and their sharing.

**Risk score.** A number from 0 to 100 for each person and service account, with its reasons,
from their recent activity against their own usual and their team's. Above 80, everything they do
is challenged. See [Risk scores](/concepts/risk/).

**SCIM.** The standard your IdP uses to send awzy-context your people and groups, and to remove
people who leave.

**Security key.** A WebAuthn credential, such as a hardware key or a passkey, that a person uses to
pass a verification challenge.

**Sensitivity.** How much harm exposure would do: `public`, `internal`, `confidential`, or
`crown_jewels`. Set by a label, or by awzy's classifier, and never lowered. See
[sensitivity](/concepts/domain-model/#sensitivity).

**Sensitivity tier.** One of `sensitivity`'s four values. A setting that holds one says `tier`, such
as the tiers that fail open during a PDP outage.

**Service account.** Anything non-human with its own credentials: a backend service, a scheduled
job, an AI agent. It has an accountable owner, and one that isn't registered is denied.

**Subject.** See *principal*. AuthZEN and other authorization APIs use the word.

**Unclassified.** A resource that no label or classifier has placed yet. It gets your default tier,
and policies can challenge it.

**Unfamiliar territory.** Resources a principal opened that neither they nor anyone on their team
had opened in the 30 days before. A few are ordinary; more raise their risk score. See
[Risk scores](/concepts/risk/).

**Verdict.** A decision's answer: allow, deny, or challenge.

**Verification.** A challenge the person answers with their security key. No agent can pass it.

**Would deny.** A deny recorded in monitor mode: what enforcing would have blocked.

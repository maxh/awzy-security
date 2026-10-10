# 0073. awzy-challenge tells people what's waiting for them, through the client's own Slack

- Status: Accepted
- Date: 2026-10-10
- Builds on: [0054](0054-challenges-and-containments-over-grpc.md) (challenges and containments),
  [0009](0009-no-vendor-access-to-client-data.md) (no vendor access to client data)

## Context

A challenge only helps if the person who has to answer it knows it exists. Today nobody is told:
the requester sees a challenge QID in an agent's tool error, and approvers see a request only if
they happen to open the client portal's Challenges page. An approval can sit unanswered until it
expires a day later, and a containment, automated or not, reaches the security team only when
someone looks.

We also discussed a mobile app. A native app's push notifications have to go through a relay
signed with the app publisher's keys, which would tell awzy when and for whom challenges happen.
An installable web app with Web Push wouldn't. That decision is deferred. What both need, and
what's needed regardless, is a way for awzy to decide who to tell about what, and to tell them
reliably.

## Decision

- **awzy-challenge tells people when something waits for them:**
  - a new challenge that needs a justification or a security key: the person it went to;
  - a new challenge that needs an approval: the resource's owner, when the PDP knows one (it
    passes the owner from the resource's entity), and the security team;
  - an approval or a rejection: the person the challenge went to, so they know to retry or not;
  - a containment, enforced or only recorded in monitor (0066): the security team, never the
    contained person.
- **The first channel is the client's own Slack.** The client installs a Slack app in their
  workspace and gives the install its bot token, which stays in the install's secrets.
  awzy-challenge sends people direct messages, found by email, and posts the security team's
  messages to one channel. Nothing goes through awzy: the messages leave the install for the
  client's own Slack, as their IdP and model service already do. Email follows as a second channel
  for people and teams without Slack.
- **Messages carry little and link to the portal.** A message says what's waiting (the kind of
  challenge, who asked, the action and the resource's ID) and links to the client portal, where
  the person signs in to answer. It never includes document contents, justifications, or the
  evidence behind a containment, and never offers a one-click approval: an approval still needs
  sign-in and the PDP's `approve` check.
- **They're recorded before they're sent.** awzy-challenge writes each notification to a
  `notifications` table right after the change that causes it, and a worker sends from there,
  retrying with backoff up to eight times. A slow or failing Slack never delays a decision. Any
  replica can send: each claims rows with `FOR UPDATE SKIP LOCKED`.
- **A person's address is their `userName` when it's an email address**, as Okta and Entra ID
  usually send it. Someone whose `userName` isn't an email isn't messaged directly, and the
  security team still is. Storing SCIM's `emails` in awzy-context would lift that limit.
- **Off until configured.** With no Slack token, nothing is recorded or sent.

Rejected: writing the notification in the same transaction as the challenge. It would be atomic,
but it would put notification knowledge in every store operation; recording it right after, from
the same request, loses a message only if the process dies in between. Rejected: an awzy-run push
service. That's the native-app question above, and it stays open.

## Consequences

- The client portal needs a public URL for the links (`AWZY_CHALLENGE_PORTAL_URL`, from the
  chart's `clientPortal.publicUrl`).
- Blocked tool calls should carry the same link, and the Challenges page should work on a phone:
  both come next, with email.
- A mobile app, if it comes, adds a channel here rather than a second way of deciding who to tell.

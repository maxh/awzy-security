# 0122. Email notifications through the client's SMTP relay

- Status: Accepted
- Date: 2026-10-13
- Amends: [0073](0073-awzy-challenge-tells-people-whats-waiting-for-them.md) (notifications through the client's Slack): email is a second channel beside Slack

## Context

0073 made the client's Slack the first way awzy-challenge tells people what's waiting for them,
and said email would follow for people and security teams without Slack. A company on Teams, or
with contractors outside its Slack, today has nobody told unless someone opens the Challenges
page.

## Decision

- **Email goes through an SMTP relay the client provides:** their mail provider's, or on AWS,
  Amazon SES's SMTP interface. SMTP works with every provider and every cloud, so awzy needs no
  per-provider API, and the messages leave the install for the client's own relay, as Slack's do
  for their workspace. The relay's URL, with its credentials, is a Secret in the install.
- **The connection is encrypted.** The relay URL is `smtps://` (TLS from the start) or `smtp://`
  with `?tls=required` (STARTTLS). Plain SMTP is refused except to a relay on the same host, so
  credentials and messages never cross a network in the clear.
- **An email says what the Slack message says.** Its body is the same text, with the same link
  into the portal; its subject names only the kind of thing waiting, not the request. The limits
  of 0073 hold: no document contents, justifications, or evidence, and no one-click approval.
- **People are emailed at the address awzy-context has for them**, the same one Slack looks them
  up by: the email their IdP provisioned over SCIM, else their `userName` when it's an address.
  The security team gets one configured address, a mailbox or a list.
- **Each channel has its own record.** A notification is written to `notifications` once per
  configured channel, with a `channel` column, and the worker sends each through its channel. A
  failing relay retries its own rows with backoff and never resends what Slack already delivered.
  With both channels on, people hear on both; the client chooses by configuring one or both.

Rejected: a provider API (SES, SendGrid) per cloud. Each would be another client and another set
of credentials to support, for what SMTP already does everywhere. Rejected: picking one channel
per person. awzy doesn't know who reads Slack, and a person who gets both loses nothing.

## Consequences

- The chart's `challenge.email` (`relaySecret`, `from`, `securityAddress`) and the bootstrap
  module's `email` variable turn it on; with neither Slack nor email, nothing is recorded.
- Teams, if it comes, is a third channel in the same table.

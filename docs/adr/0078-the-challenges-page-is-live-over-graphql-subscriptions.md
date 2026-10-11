# 0078. The Challenges page is live, over GraphQL subscriptions

- Status: Accepted
- Date: 2026-10-10
- Builds on: [0054](0054-challenges-and-containments-over-grpc.md) (awzy-challenge and its
  containment feed) and [0020](0020-the-portal-is-authorized-by-the-pdp.md) (the portal asks the
  PDP)

## Context

The client portal's pages changed only when reloaded. On the Challenges page that's the wrong
behavior: a person waiting for an approval, or an agent's owner answering for it, has to keep
reloading to learn the request was approved, and the security team doesn't see a containment
until it reloads either. The Relay client already speaks `graphql-ws`; the backend had no
subscriptions and nowhere for a WebSocket to connect.

## Decision

- **Two subscriptions in the client portal:** `challengeUpdated`, each challenge as it opens or
  changes, the viewer's own, or every one for those the PDP lets administer challenges; and
  `containmentsChanged`, the active list whenever it changes, for those who administer
  challenges. Each is authorized like any portal operation (0020), once, when it starts.
- **From awzy-challenge's streams.** A trigger notifies `awzy_challenges` with each new or changed
  challenge's ID; awzy-challenge's feed broadcasts it, and a new `WatchChallenges` stream (an
  additive RPC) sends the challenge, filtered to one answerer when asked. Containments reuse
  `WatchContainments`, which the portal may now call.
- **A WebSocket on `/graphql`**, signed in by the bearer token in its first message
  (`connectionParams.authorization`), since a browser can't set headers on one; the same check as
  every request. GET without an upgrade still serves GraphiQL.
- **The page refreshes on an event** rather than patching the store: one refetch for a burst of
  changes, which keeps the lists and counts right without per-event logic.

## Consequences

- Each open Challenges page holds one WebSocket to the portal and a gRPC stream from the portal to
  awzy-challenge per subscription.
- A token that expires during a long-lived connection stays accepted until the socket closes.
  Reconnecting signs in again.
- Other pages can subscribe the same way when they need to.

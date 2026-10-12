# 0112. The TypeScript SDK: in-process checks over the PDP's JSON API, with a budget per call

- Status: Accepted
- Date: 2026-10-12
- Implements: [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (the first SDK, in the design partner's language)
- Builds on: [0030](0030-gateway-outage-fallback.md) (outage fallback), [0084](0084-awzys-own-surfaces-record-agent-activity.md) (activity), [0110](0110-apps-mint-tokens-for-their-own-agents.md) (app-minted agents), [0111](0111-agents-wait-for-challenges-in-awzy.md) (waiting for a challenge)

## Context

The first design partner's app is TypeScript. Its agents call typed tool functions from scripts,
and each call should be checked: the agent as principal, the person it acts for, the tool function
as the action, and the tenant or entity as the resource. ADR 0045 sets the rules every SDK
follows. This one decides where the SDK lives, how it talks to the PDP, and how it falls back.

## Decision

- **It lives in `authz/sdks/typescript`** as `@awzy/sdk`: client-side code, built and tested with
  the rest. It ships TypeScript a modern runtime or bundler can run as is, with one dependency,
  `jose`, for token validation.
- **It speaks the PDP's JSON API**, `POST /v1/access/evaluation` and `POST /v1/access/challenge`.
  These carry the same messages as `awzy.authz.v1`, and need no gRPC stack in the app. A service
  token for the PDP comes from a function the app gives it, read fresh for each call.
- **It validates the caller's token itself** (0045, rule 1), with the same rules as awzy-gateway:
  `act` makes a service account act for the token's subject, a client-credentials token is a
  service account on its own, and anything else is a person. An app's own agents' tokens are
  accepted from the apps it's told to trust, under each one's prefix (0110).
- **Each check has a latency budget** (default 250 ms). When the PDP doesn't answer within it, or
  can't be reached, the check falls back as the gateway does (0030). In monitor mode it allows. In
  enforce mode it fails closed unless the app set the action to fail open, the resource's tier is
  known and isn't `confidential` or `crown_jewels`, and the PDP allowed the same request within
  the cached-allow TTL. The portal's fail-open tiers can only narrow this, never widen it.
- **A result says what to do** as a typed union: allowed (with the mode and what the verdict
  would have been), denied, challenged (with the challenge's QID and a wait that resolves when
  the person answers, 0111), or unavailable. Each carries the decision's QID, which it also hands
  to the app's logger.
- **`checkMany` checks several calls at once**, each its own decision, sharing one budget.
- **Calls it allows can be recorded as activity** in awzy-intake, batched and best effort, as the
  gateway records tool calls (0084).

Rejected: gRPC from the SDK. It would add a native or HTTP/2 dependency to every app for no gain
in what's decided. Rejected: a batch endpoint in the PDP for `checkMany`. Parallel requests over
one keep-alive connection are fast enough, and leave the PDP's hot path as it is.

## Consequences

- The JSON API is now a contract like the proto: a change that breaks it needs a new path.
- The conformance scenarios run against the SDK with a fake PDP that answers as the real one
  does. Running them against a real PDP and the gateway together is the next step.
- The SDK doesn't fetch the portal's fail-open tiers or trusted apps; the app passes them in.

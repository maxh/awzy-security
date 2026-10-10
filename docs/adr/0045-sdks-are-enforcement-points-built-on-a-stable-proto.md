# 0045. SDKs are enforcement points built on a stable proto, starting with a design partner's language

- Status: Accepted
- Date: 2026-10-09
- Implements part of: [0003](0003-one-pdp-for-every-access-path.md) (SDKs as an access path)

## Context

0003 and 0019 name language SDKs as the enforcement point after awzy-gateway, for apps a client
writes itself. The PDP's API is one gRPC call, `awzy.authz.v1.Authorizer/Check`, plus the
AuthZEN-style HTTP endpoint, so any language can generate a client in minutes. A generated
client isn't an enforcement point, though. Each app would have to get right what the gateway
already does: validating tokens and delegation (0008, 0016), monitor mode (0007), challenges,
the outage fallback (0030), and, since 0044, sending resource IDs rather than attributes. Done
badly once per app, that's where enforcement would quietly fail open.

No design partner app exists yet, so which language and framework comes first is unknown. And
once code outside this repository is generated from `authz/libs/proto`, changes to it can break
clients we don't control.

## Decision

**The protobuf files are a public contract from now on.**

- `buf.yaml` at the repository root covers `authz/libs/proto`. CI's `proto` job runs `buf lint`
  (the STANDARD rules), `buf format`, and, on pull requests, `buf breaking` against `main` with
  the FILE rules: no change that breaks generated code or the wire.
- `Authorizer` keeps its name despite buf's `Service` suffix rule. Renaming it would change the
  path every enforcement point calls.
- A breaking change means a new package version (`awzy.authz.v2`) served alongside v1, not an
  edit to v1.

**An awzy SDK is an enforcement point, not a client.** Every SDK, in any language:

1. **Validates the caller's token** (signature, issuer, audience, expiry) even when the app
   already did, and builds the subject from validated claims only, never from a bare user ID
   (0008). An RFC 8693 `act` claim makes the caller a service account acting for the token's
   subject (0016).
2. **Sends resource IDs, not attributes** (0044). Attributes are sent only for entity types
   awzy-context doesn't hold.
3. **Applies the mode** from each response (0007). In monitor mode it allows and records what
   would have happened. A response it can't read applies the verdict, which fails safe.
4. **Surfaces challenges** as a typed result the app can show to the user and retry, never as a
   plain deny or a plain allow.
5. **Falls back as the gateway does when the PDP can't be reached** (0030). Monitor mode keeps
   allowing. In enforce mode it fails closed unless the resource's tier is in the client's
   fail-open tiers and the PDP allowed the same request within the cached-allow TTL.
   `confidential` and `crown_jewels` never fail open.
6. **Records the decision ID** in the app's logs and traces, so a decision can be explained from
   the app's side.
7. **Ships with a conformance suite**: the same scenarios run against every SDK and the gateway,
   with a real PDP, including outage and monitor mode.

**Order:** the first SDK is in the first design partner's language, as middleware for their
framework. Without a partner by the time one is needed, TypeScript for Node, which the portals
already use. One SDK at a time, each passing the conformance suite before the next is started.

Rejected: publishing generated clients for several languages now. They would carry none of the
above, and apps built on them would be enforcement points awzy can't vouch for. Rejected:
building SDKs before the gateway's behavior settles. The gateway is where these rules are proven
first, and the SDKs port them.

## Consequences

- Every proto change now passes `buf breaking`. Fixing a mistake in a shipped field means adding
  a field and deprecating the old one.
- Each SDK reimplements token validation and the outage fallback in its language. The
  conformance suite is what keeps them equal to the gateway's Rust.
- Until the first SDK exists, apps a client writes can call the PDP directly, but awzy doesn't
  support that as enforcement.

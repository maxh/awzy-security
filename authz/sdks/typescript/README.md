# @awzy/sdk

awzy's TypeScript SDK: an enforcement point in a client's own app (ADR 0045, 0112). It validates
callers' tokens, asks awzy-pdp within a budget, applies the mode, surfaces challenges as a typed
result to wait on (ADR 0111), falls back as awzy-gateway does, and records activity.

- `src/`: the SDK. `client.test.ts` holds the conformance scenarios, against a fake PDP.
- `example/agent.ts`: a sample agent script whose tool calls go through awzy.

How to use it is on the docs site's "Checking calls in a TypeScript app" guide.

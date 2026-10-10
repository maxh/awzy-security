# 0003. One PDP for every access path

- Status: Accepted, amended by [0019](0019-mcp-proxy-is-the-first-pep.md) (the first PEP)
- Date: 2026-10-08

## Context

The plan's first draft led with agent enforcement through an MCP proxy, arguing that it needed no
changes to client apps and that no incumbent owned that path. That mixed a sales judgment into the
architecture. Architecturally, a decision is the same whether the request came from a browser, an
API client, or an agent's tool call.

## Decision

- There is one policy decision point (PDP): awzy-pdp, a gRPC service
  (`awzy.authz.v1.Authorizer/Check`, in `authz/proto`), with an AuthZEN-style HTTP API on the same port.
- Policy enforcement points (PEPs) are interchangeable callers of it: awzy-gateway (HTTP reverse
  proxy and MCP proxy), language SDKs, and an Envoy `ext_authz` filter. They differ only in how much
  work the client does to install them and to map a request to a resource and an action.
- The first PEP is whichever access path the first design partner needs. That is still open.
- The PDP's hot path reads precomputed attributes and evaluates policy. It never calls an LLM or
  ClickHouse inline. Target: p99 under 10 ms inside the client network.

## Consequences

- Phase 1 builds the PDP and one PEP. The other PEPs reuse the same API.
- Every PEP needs a fallback for when the PDP is unreachable: a signed fallback bundle that maps
  what the PEP can see to a sensitivity tier, failing closed for anything unlisted.

---
title: Putting MCP servers behind awzy-gateway
description: Check every AI agent's tool call before it reaches your MCP servers.
---

awzy-gateway is an MCP proxy. Agents connect to it instead of to your MCP servers, and it checks
each tool call with the PDP before forwarding it. A call that's denied or challenged never
reaches the server; the agent gets a tool error saying why. Your MCP servers need no changes.

The Helm chart doesn't deploy awzy-gateway yet. Run its image as its own Deployment in the
`awzy-system` namespace, with the settings below.

## How a call is checked

For each `tools/call`, the gateway asks the PDP:

1. whether the caller may `call` the tool, as `McpTool::"<server>/<tool>"`, which carries the tool's
   sensitivity;
2. for a tool you've said acts on one resource, whether the caller may `read` (or `write`) that
   resource, as `Resource::"<id>"`, which the PDP loads from awzy-context with its sensitivity,
   owner, and sharing.

Both must allow. A challenge comes back with its challenge ID, and the agent retries the same
call once the person has passed it. The caller's token is never forwarded to your server.

## Configuration

Point `AWZY_GATEWAY_CONFIG` at a JSON file, `AWZY_PDP_ADDR` at awzy-pdp, and `AWZY_CHALLENGE_ADDR`
at awzy-challenge (for containments during a PDP outage; see below).

```json
{
  "token": {
    "issuer": "https://your-tenant.okta.com/oauth2/default",
    "audience": "awzy-gateway",
    "subject_claim": "email",
    "jwks_url": "https://your-tenant.okta.com/oauth2/default/v1/keys"
  },
  "upstreams": [
    {
      "name": "drive",
      "url": "http://drive-mcp.tools.svc:8080/mcp",
      "default_sensitivity": "internal",
      "tools": { "search": "internal", "read_document": "internal", "share_document": "confidential" },
      "resource_tools": {
        "read_document": { "argument": "id" },
        "share_document": { "argument": "id", "action": "write" }
      }
    }
  ],
  "service_account_registry": "awzy-context",
  "outage": { "fail_open_tiers": ["public", "internal"], "cached_allow_ttl_secs": 300 }
}
```

- **`token`:** how the gateway validates the bearer token an agent presents.
  - `issuer` and `audience` must match the token.
  - The signing key comes from `jwks_url` (your IdP's keys, RS256 or ES256) or
    `rs256_public_key_file`.
  - `subject_claim` is the claim that holds the person's ID as your IdP provisions it over SCIM.
    It's `sub` by default; with Okta or Entra ID it's usually `email` or `preferred_username`,
    since their `sub` is opaque.
  - `jwks_ca_file` trusts a private CA.
- **`upstreams`:** one per MCP server. Agents reach each at `/mcp/<name>`.
  - `url` is the server's streamable HTTP endpoint.
  - `tools` gives each tool's sensitivity, and `default_sensitivity` covers the rest. Your policies
    see it as `resource.sensitivity` on the tool, and the base policies let everyone call
    `public` and `internal` tools.
  - `resource_tools` names the tools that act on one resource: `argument` is the tool argument
    holding the resource's ID in awzy-context, and `action` is `read` (the default) or `write`.
- **`service_account_registry`:** `awzy-context` checks agents against the service accounts
  registered on the client portal's Directory page, and denies any other.
- **`outage`:** see below.

## Who's calling

The token says who the request is for:

- **A person**, through an agent that uses their token: the principal is that person.
- **An agent acting for a person**, with an RFC 8693 `act` claim (`{"act": {"sub": "sales-genie"}}`):
  the agent is the principal, `sub` is the person, and both are checked. See
  [People, agents, and delegation](/concepts/principals-and-delegation/).
- **An agent on its own authority**, with a client-credentials token for a registered service
  account.

A boolean `awzy_human_present` claim tells the PDP whether a person is there to answer a
challenge.

## When the PDP can't be reached

An enforcing gateway denies every call while the PDP is unreachable, with one exception you
choose. A tool listed by name in a tier in `outage.fail_open_tiers` can still be called by
someone the PDP allowed to make that same call within the last `cached_allow_ttl_secs`.
`confidential` and `crown_jewels` can never fail open, whatever the file says. The gateway follows
awzy-challenge's list of who is contained, so it never replays an allow for someone contained
before the outage.

## Connecting an agent

Give the agent the gateway's URL for the server, `https://<gateway>/mcp/<name>`, and a token from
your IdP for the gateway's audience. Register each agent's service account, with its owner, on the
client portal's Directory page first, or every call it makes is denied.

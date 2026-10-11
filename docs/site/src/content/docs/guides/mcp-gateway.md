---
title: Putting MCP servers behind awzy-gateway
description: Check every AI agent's tool call before it reaches your MCP servers.
---

awzy-gateway is an MCP proxy. Agents connect to it instead of to your MCP servers, and it checks
each tool call with the PDP before forwarding it. A call that's denied or challenged never
reaches the server; the agent gets a tool error saying why. Your MCP servers need no changes.

Turn it on in the cluster layer's Terraform, with the `mcp_gateway` variable (see
[Installing awzy](/guides/install/), step 5). Argo CD then runs it in `awzy-system`, beside the
PDPs.

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

```hcl
mcp_gateway = {
  # Where agents outside the cluster reach it; it needs a DNS record to the load balancer.
  # null keeps it in-cluster, at http://awzy-gateway.awzy-system.svc:8080.
  hostname = "mcp.example.com"

  upstreams = [
    {
      name  = "drive"
      url   = "http://drive-mcp.tools.svc:8080/mcp"
      tools = { search = "internal", read_document = "internal", share_document = "confidential" }
      resource_tools = {
        read_document  = { argument = "id" }
        share_document = { argument = "id", action = "write" }
      }
    }
  ]

  # Tiers whose tools may replay a recent allow while the PDP is unreachable (see below).
  fail_open_tiers = ["public", "internal"]
}
```

- **`upstreams`:** one per MCP server. Agents reach each at `/mcp/<name>`.
  - `url` is the server's streamable HTTP endpoint.
  - `tools` gives each tool's sensitivity, and `default_sensitivity` covers the rest. It's
    `confidential` unless you set it, so a tool the server adds later isn't open to everyone
    before you've classified it. Your policies see it as `resource.sensitivity` on the tool, and
    the base policies let everyone call `public` and `internal` tools.
  - `resource_tools` names the tools that act on one resource: `argument` is the tool argument
    holding the resource's ID in awzy-context, and `action` is `read` (the default) or `write`.
- **`fail_open_tiers`:** see [below](#when-the-pdp-cant-be-reached).

Agents are checked against the service accounts registered on the client portal's Directory
page, and any other is denied.

### Tokens

The gateway accepts the same tokens as the client portal: from your IdP (the portal's issuer and
keys, RS256 or ES256), for the audience `awzy`. A person is identified by the token's `sub`, which
has to be their user name as your IdP provisions it over SCIM. Choosing another claim, such as
`email` for an IdP whose `sub` is an opaque ID, isn't available in an install yet.

Because the two accept the same tokens, a token you give an agent for the gateway also works on
the client portal's API. See
[An agent using a person's own credentials](/concepts/principals-and-delegation/#an-agent-using-a-persons-own-credentials).

## Who's calling

The token says who the request is for:

- **A person**, through an agent that uses their token: the principal is that person, and the
  check carries `context.client == "mcp"`. See
  [An agent using a person's own credentials](/concepts/principals-and-delegation/#an-agent-using-a-persons-own-credentials).
- **An agent acting for a person**, with an RFC 8693 `act` claim (`{"act": {"sub": "sales-genie"}}`):
  the agent is the principal, `sub` is the person, and both are checked. See
  [People, agents, and delegation](/concepts/principals-and-delegation/).
- **An agent on its own authority**, with a client-credentials token for a registered service
  account.

A boolean `awzy_human_present` claim tells the PDP whether a person is there to answer a
challenge.

## When the PDP can't be reached

An enforcing gateway denies every call while the PDP is unreachable, with one exception you
choose. A tool listed by name in `tools`, in a tier in `fail_open_tiers`, can still be called by
someone the PDP allowed to make that same call within the last five minutes. `confidential` and
`crown_jewels` can never fail open, whatever the configuration says. The gateway follows
awzy-challenge's list of who is contained, so it never replays an allow for someone contained
before the outage.

## Connecting an agent

Give the agent the gateway's URL for the server, `https://<gateway>/mcp/<name>`, and a token from
your IdP for the audience `awzy`. Register each agent's service account, with its owner, on the
client portal's Directory page first, or every call it makes is denied.

The gateway doesn't publish OAuth sign-in metadata yet, so an MCP client can't sign in to it on
its own. Configure the token as an `Authorization: Bearer` header in the client. For Claude Code,
for example:

```sh
claude mcp add --transport http drive https://mcp.example.com/mcp/drive \
  --header "Authorization: Bearer $TOKEN"
```

A token you get for yourself makes the agent you: see
[An agent using a person's own credentials](/concepts/principals-and-delegation/#an-agent-using-a-persons-own-credentials).
An agent with its own identity uses a token-exchange or client-credentials token instead.

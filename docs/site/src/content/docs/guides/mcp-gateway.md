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
      name = "drive"
      url  = "http://drive-mcp.tools.svc:8080/mcp"
      resource_tools = {
        read_document  = { argument = "id" }
        share_document = { argument = "id", action = "write" }
      }
    }
  ]
}
```

- **`upstreams`:** one per MCP server. Agents reach each at `/mcp/<name>`.
  - `url` is the server's streamable HTTP endpoint.
  - `resource_tools` names the tools that act on one resource: `argument` is the tool argument
    holding the resource's ID in awzy-context, and `action` is `read` (the default) or `write`.

## Classifying tools

Each tool's sensitivity tier is set on the client portal's Enforcement page, under **During a PDP
outage**: by name, as `<server>/<tool>`, and a default for each server's other tools. A tool you
haven't classified is `confidential`, so one a server adds later isn't open to everyone before
you've looked at it. Your policies see the tier as `resource.sensitivity` on the tool, and the
base policies let everyone call `public` and `internal` tools.

Saving asks for your security key, as switching the enforcement mode does. awzy signs each
version, and every gateway checks the signature before using it, within seconds.

Agents are checked against the service accounts registered on the client portal's Directory
page, and any other is denied.

### Tokens

The gateway accepts tokens from the client portal's IdP (its issuer and keys, RS256 or ES256), for
an audience of its own: `awzy-gateway`, or the `audience` you set in `mcp_gateway`. The client
portal doesn't accept them, so a token you give an agent for the gateway can't answer challenges
or approve anything there.

A person is identified by the claim your install names as `client_portal.subject_claim`: `sub`
by default, or a claim such as `email` when your IdP's `sub` is an opaque ID. Either way it has to
hold their user name as your IdP provisions it over SCIM.

## Who's calling

The token says who the request is for:

- **A person**, through an agent that uses their token: the principal is that person, and the
  check carries `context.client == "mcp"` and the MCP client's name as `context.mcp_client`. See
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
choose on the Enforcement page. A tool classified by name, in a tier you let fail open, can still
be called by someone the PDP allowed to make that same call within the last five minutes.
`confidential` and `crown_jewels` can never fail open. The gateway follows
awzy-challenge's list of who is contained, so it never replays an allow for someone contained
before the outage.

## Connecting an agent

Give the agent the gateway's URL for the server, `https://<gateway>/mcp/<name>`. Register each
agent's service account, with its owner, on the client portal's Directory page first, or every
call it makes is denied.

The gateway publishes OAuth protected resource metadata for each server, at
`https://<gateway>/.well-known/oauth-protected-resource/mcp/<name>`, and points to it when it
refuses a call without a token. An MCP client that supports it finds your IdP there and signs in
on its own, if your IdP lets the client register or knows it already, and issues tokens for the
gateway's audience.

Otherwise, get a token for the gateway's audience and configure it as an `Authorization: Bearer`
header in the client. For Claude Code, for example:

```sh
claude mcp add --transport http drive https://mcp.example.com/mcp/drive \
  --header "Authorization: Bearer $TOKEN"
```

A token you get for yourself makes the agent you: see
[An agent using a person's own credentials](/concepts/principals-and-delegation/#an-agent-using-a-persons-own-credentials).
An agent with its own identity uses a token-exchange or client-credentials token instead.

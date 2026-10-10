# 0050. An MCP tool call that acts on a resource is also checked on that resource

- Status: Accepted
- Date: 2026-10-09
- Amends: [0019](0019-mcp-proxy-is-the-first-pep.md)

## Context

awzy-gateway checks every MCP `tools/call` as `call` on `McpTool::"<server>/<tool>"`, with a
sensitivity the gateway's config gives each tool (0019). That decides whether an agent may use
`read_document` at all, not whether it may read this document. The work since then is about
the second question: the PDP loads a resource by ID from awzy-context (0044), with who is
assigned to it (0046) and its classified tier (0047). None of that applies to MCP until the
gateway asks about the resource.

The demo's agent scenarios (the rogue agent reading the strategic plan) run through the gateway,
and its activity driver (demo plan step 5) needs an MCP server with real documents behind it.

## Decision

- An upstream's config can list **resource tools**: a tool name, the argument that holds the
  resource's ID, and the action (`read` by default, or `write`):

  ```json
  "resource_tools": { "read_document": { "argument": "id" }, "list_folder": { "argument": "id" } }
  ```

- A call to a resource tool is checked twice: `call` on the tool, as before, then the action on
  `Resource::"<argument value>"`. The gateway sends only the ID; the PDP loads the rest. Both
  checks must allow. Each is logged as its own decision, and in monitor mode both are recorded
  and the call goes through.
- A call that lacks the argument is rejected (JSON-RPC invalid params) without reaching the
  upstream.
- **Outages** follow 0030 per check. The resource check replays only a recent allow for the
  same caller, tool, and resource, so a fail-open tool doesn't open every document behind it.
- For the demo, awzy-synthetic serves **larkspur-drive** (`awzy-synthetic drive`): an MCP server
  with `list_folder` and `read_document` over the documents in awzy-context. It stands in for a
  client's Drive or SharePoint MCP server and does no authorization of its own.

Rejected: inferring the resource from tool results, or from any argument that looks like an ID.
A guess that misses fails open. Rejected: one combined check with the tool as context. The tool
and the document are different things with their own policies, and two decisions explain
themselves better than one.

## Consequences

- Each resource-tool call costs two PDP checks. The second may wait on awzy-context for a
  resource the PDP hasn't cached (0044).
- Tools that act on several resources, or that search, are still checked only as tools. Search
  results can name documents the caller couldn't open; filtering them is later work.
- The resource IDs a tool takes must be the IDs awzy-context holds. For real servers that means a
  connector and an MCP server that agree on IDs. The config can't translate them yet.

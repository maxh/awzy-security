# 0081. Agents get tokens of their own audience, MCP clients can find where to sign in, and say which client they are

- Status: Accepted
- Date: 2026-10-11
- Builds on: [0016](0016-human-and-serviceaccount-principals.md) (delegation), [0019](0019-mcp-proxy-is-the-first-pep.md)
  (awzy-gateway), [0020](0020-the-portal-is-authorized-by-the-pdp.md) (the portal's access), and
  [0065](0065-agents-approve-people-verify.md) (agents as approvers)

## Context

Coding agents such as Claude Code usually run with their person's own token, so awzy sees the
person. Reviewing what that means turned up five gaps:

- **One audience for everything.** The client portal's GraphQL API, its MCP endpoint, and
  awzy-gateway in the chart all took tokens for `awzy`. A token a person handed an agent for MCP
  also drove the portal's API as them: it could answer their justification challenges, and
  approve what they may approve.
- **No way for an MCP client to sign in.** awzy-gateway published no OAuth protected resource
  metadata (RFC 9728), so an MCP client couldn't find the IdP; the only way in was pasting a
  token into a header.
- **Every client looked the same.** awzy-gateway sent `context.client == "mcp"` for every call,
  so policy couldn't tell Claude Code from any other MCP client.
- **Only security admins saw approvals.** The portal listed waiting approvals only to those who
  `administer` challenges, so a resource's owner whom policy lets approve couldn't see or open the
  request they'd been told about, and the notification's `?challenge=` link opened nothing.
- **Installs couldn't name the subject claim**, nor reach the portal's MCP endpoint at all: the
  chart passed neither `AWZY_AUTH_SUBJECT_CLAIM` nor `AWZY_MCP_ALLOWED_HOSTS`.

## Decision

- **Three audiences.** The portal's own tokens are for `awzy`, its MCP endpoint's for `awzy-mcp`
  (`AWZY_MCP_AUDIENCE`), and awzy-gateway's for `awzy-gateway`. Each refuses the others' tokens.
  All three, and the subject claim, are bootstrap settings, since IdPs name audiences their own way
  (Entra's are app ID URIs). The dev IdP issues any of the three on request.
- **OAuth metadata.** awzy-gateway serves RFC 9728 metadata for each upstream at
  `/.well-known/oauth-protected-resource/mcp/<upstream>` and names it in its 401's
  `WWW-Authenticate`, when it knows its public URL (the chart sets it from the hostname). The
  portal's metadata now describes `/mcp`, at that path and at the root.
- **The client's name, as a hint.** awzy-gateway reads `clientInfo.name` from each session's
  `initialize`, keeps it by the session ID the upstream assigns and the principal that started it,
  and sends it with each `tools/call` as `context.mcp_client`. It's what the client says it is, so
  policy can use it to ask for more, never to grant. `client` stays `mcp`, so a `Human` arriving
  that way still reads as software holding a person's token. A PDP drops `mcp_client` for a bundle
  whose schema doesn't declare it, as it does `assigned`.
- **Approvers see what they may approve.** The portal's `challengesToApprove` lists pending
  approvals the signed-in principal may give: it asks the PDP the same `approve` check
  awzy-challenge makes before recording one, and leaves out requests the viewer made, is acted
  for in, or answers. Such an approver may also open the challenge by ID. The Challenges page shows
  them under *Waiting for your approval*, and highlights the challenge a link names.

Rejected: telling agents apart by `User-Agent`. It isn't part of MCP, and clients set it
inconsistently; `clientInfo` is in the protocol.

## Consequences

- An install's IdP needs an audience, or an app, for each of the three. Tokens with more than one
  audience still open everything they name.
- The approver list costs one PDP check per pending approval each time the page loads, up to 500.
  If installs have more waiting than that, it needs an index of approvers instead.
- `mcp_client` is known only for upstreams that keep sessions; a stateless upstream assigns no
  session ID to key it by.
- An agent with the person's token for the portal itself can still answer as them. Only a
  security key proves a person; the docs say so.

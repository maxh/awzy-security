---
title: Agent builders
description: "Building AI agents and services that work under awzy: registering them, acting for people, and handling challenges."
---

You build the AI agents and backend services that act in your company's systems. awzy checks
every action they take, on the resource it touches.

## 1. Register the service account

Every agent or service gets a **service account** on the client portal's Directory page:

- **ID:** as its tokens name it: their `sub`, or `act.sub` when it acts for a person.
- **Owner:** the person accountable for it. They answer its challenges when it acts on its own.
- **AI agent:** whether it's an AI system. Policies can treat AI differently, for example never
  letting an unattended agent touch `crown_jewels`.
- **Groups:** the groups policies should see it in.

An account that isn't registered is denied everything.

### When your app mints its own agents

If your app creates agents as it goes, and signs their tokens itself rather than getting them from
your IdP, an admin can trust it as a token issuer instead. On the Directory page's **Apps** tab
they give its name, the `iss` its tokens carry, its JWKS URL (RS256 or ES256 keys, over HTTPS),
and the `aud` its tokens must carry.

The app then signs tokens only for service accounts whose ID starts with its name and a dot: an
app named `acme-crm` signs for `acme-crm.renewals-bot`, and nothing else. A token it signs for a
person on their own, or for any other service account, is refused. To act for a person, it puts
the person's user name in `sub` and the agent in `act.sub`, as a token exchange would, and awzy
checks both.

The app registers and retires its agents itself, with a token it signs for itself, whose `sub` is
its name:

```sh
# Register or change an agent. Owner: the accountable person, provisioned by your IdP.
curl -X PUT https://<portal>/api/v1/agents/acme-crm.renewals-bot \
  -H "authorization: Bearer $APP_TOKEN" -H "content-type: application/json" \
  -d '{"owner": "dana@example.com", "ai": true, "groups": ["sales"], "description": "Drafts renewals"}'

# Its agents, then retire one.
curl https://<portal>/api/v1/agents -H "authorization: Bearer $APP_TOKEN"
curl -X DELETE https://<portal>/api/v1/agents/acme-crm.renewals-bot -H "authorization: Bearer $APP_TOKEN"
```

Each change is recorded with the app as who made it. An agent that isn't registered yet is denied
like any other, and the denial is in the decision log. Enforcement points pick up a newly trusted
app, or stop trusting a removed one, within about ten seconds.

## 2. Act for a person when there is one

When your agent works for someone, get a token that says so: an OAuth token exchange
(RFC 8693) with the person as `sub` and your agent as `act.sub`. awzy then checks both the agent and
the person, and the stricter verdict wins, so your agent never gets more than the person could.
Challenges go to that person.

When it works on its own, such as a scheduled job, use its own client-credentials token. Then
challenges go to its owner. See [People, agents, and delegation](/concepts/principals-and-delegation/).

Set the token's `awzy_human_present` claim to `false` when nobody is there to answer a challenge.
Some policies ask for more when nobody is present.

Set its `awzy_work_item_id` claim to the task the agent was given, in the ID your tracker's
connector uses (`jira:SALES-12`). awzy then counts the agent as assigned only to what that task
links, not to everything its person is working on, and every decision records the task. A task
awzy doesn't know, or one that's closed, assigns nothing.

### Not with the person's own token

The easy path, signing your agent in with its person's own token, makes it that person to awzy.
Nothing marks its requests as an agent's beyond the MCP client's name, and it's assumed to have
its person at the keyboard. Coding
agents such as Claude Code often run this way. awzy still decides what passes through an
enforcement point, and a security key challenge still needs the person, but policies, risk
scores, and challenges can no longer tell the agent from its person. Use a token exchange
instead. See
[An agent using a person's own credentials](/concepts/principals-and-delegation/#an-agent-using-a-persons-own-credentials).

## 3. Call tools through awzy-gateway, or check yourself

- **MCP tools:** point your agent at awzy-gateway instead of the MCP server:
  `https://<gateway>/mcp/<server>`. Every tool call is checked before it runs. See
  [Putting MCP servers behind awzy-gateway](/guides/mcp-gateway/).
- **Your own code:** ask the PDP directly before an action, over gRPC or HTTP, and apply the
  verdict. See the [decision API](/reference/decision-api/).

## 4. Handle challenges and denials

A blocked MCP tool call comes back as a tool error, with `_meta`:

| `_meta` field         | Meaning                                                             |
| --------------------- | ------------------------------------------------------------------- |
| `awzy/verdict`        | `deny`, `challenge`, or `unavailable` (awzy couldn't reach its PDP) |
| `awzy/decision_qid`   | The decision's ID, for the security team to explain                 |
| `awzy/challenge_qid`  | For a challenge: the one to answer                                  |
| `awzy/challenge_wait` | For a challenge: where to wait for its answer, on the same gateway  |

What your agent should do:

- **Challenge:** tell its person what's needed and where to answer it (the client portal's
  Challenges page). Retry the **same** call once it's passed. A passed challenge holds for that
  exact request, for a short while. Don't try to work around it with a different resource or
  tool.
- **Deny:** stop, and tell its person, with the decision ID. Retrying won't change it.
- **Unavailable:** retry after a short wait.

### When nobody is in the agent's session

An agent running a script can't show its person a prompt. It doesn't need to: awzy sends the
challenge to the person, tells them in Slack or by email when that's set up, and they answer in the client
portal. The agent waits for the answer, then retries:

```sh
curl https://<gateway>/mcp/challenges/<challenge_qid>?wait=25 -H "authorization: Bearer $AGENT_TOKEN"
```

```json
{ "outcome": "pending", "expires_at": "2026-10-12T15:04:05Z", "grant_expires_at": null, "retry": false }
```

Each call waits up to 25 seconds. Ask again while `outcome` is `pending`. On `passed`, retry the
same call before `grant_expires_at`. On `rejected`, stop: the request is denied until
`expires_at`. On `expired`, a retry opens a new challenge. Only the agent whose call opened the
challenge, acting for the same person, gets an answer; for anyone else it isn't found. Waiting
hands the agent nothing it could replay, and an agent can't answer a challenge itself.

## 5. Test in monitor mode

While the install is in monitor, nothing is blocked, but every decision is recorded. Run your agent
through its normal work and look for its would-denies on the Decisions page. Fix its permissions
or your policies before enforcement starts.

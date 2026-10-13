---
title: Connecting your IdP
description: Provisioning people and groups over SCIM, and signing in to the client portal with OIDC, in Okta, Microsoft Entra ID, and Google Workspace.
---

awzy needs two things from your identity provider:

- **Provisioning (SCIM 2.0):** your IdP sends awzy the people and groups it should know. Policies
  name groups, and a person's groups decide what they and their agents may do. awzy keeps what
  your IdP sends and nothing else: people aren't created any other way.
- **Sign-in (OIDC):** people sign in to the client portal, and agents get tokens for its MCP
  endpoint and for awzy-gateway, from your IdP. awzy checks each token's signature against your
  IdP's keys.

The two meet in one place: a token's subject has to be the person's SCIM user name, so awzy can
tell which provisioned person signed in.

## What to have ready

From the install (step 6 of the [install guide](/guides/install/)):

- `tofu output -json scim`: the SCIM base URL, `https://<portal hostname>/scim/v2`, and its bearer
  token.
- The portal's public URL, and its sign-in callback, `https://<portal hostname>/auth/callback`.

And the bootstrap settings under `client_portal` that describe your IdP:

| Setting | What it is |
| --- | --- |
| `issuer`, `jwks_url` | Your IdP's OIDC issuer and the URL of its signing keys |
| `audience` | The audience of the portal's own tokens (default `awzy`) |
| `mcp_audience` | The audience of the tokens agents get for the portal's MCP endpoint (default `awzy-mcp`) |
| `subject_claim` | The claim that holds the person's SCIM user name, when `sub` doesn't (default `sub`) |

awzy-gateway's audience (`mcp_gateway.audience`, default `awzy-gateway`) is the third. The
audiences are kept apart so a token handed to an agent for one doesn't open the others.

## Okta

**Provisioning.** Create an app integration for awzy with SCIM provisioning on (in the app's
**General** tab, enable SCIM provisioning). Under **Provisioning → Integration**, set:

- **SCIM connector base URL:** the SCIM URL above.
- **Unique identifier field for users:** `userName`.
- **Supported provisioning actions:** push new users, push profile updates, and push groups.
- **Authentication mode:** HTTP header, with the SCIM bearer token.

Then, under **Provisioning → To App**, enable creating users, updating user attributes, and
deactivating users. Assign the people who should be known to awzy, and push the groups your
policies name (**Push Groups**).

**Sign-in.** Create an OIDC web application with the portal's callback as its sign-in redirect
URI, and an authorization server whose audience is `awzy` (or set `audience` to yours). Okta's
`sub` for a person is their Okta ID, not their user name, so set `subject_claim` to a claim that
carries the user name, such as `email` when your SCIM user names are email addresses.

## Microsoft Entra ID

**Provisioning.** Create an enterprise application for awzy (a non-gallery application), then
under **Provisioning**, set the mode to **Automatic** with:

- **Tenant URL:** the SCIM URL above.
- **Secret token:** the SCIM bearer token.

**Test connection** checks both. Under **Mappings**, keep `userPrincipalName` (or `mail`) mapped
to `userName`, and make sure groups are provisioned. Assign the users and groups awzy should
know, and turn provisioning on. Entra provisions on its own schedule, every 40 minutes or so;
**Provision on demand** sends one person at once.

**Sign-in.** Register an application for the portal with the callback as a web redirect URI, and
expose an API for its tokens. Entra's audiences are application ID URIs (`api://…`), so set
`audience`, `mcp_audience`, and `mcp_gateway.audience` to yours. Entra's `sub` is opaque and
different for every application, so set `subject_claim` to `email` or `upn`, whichever matches
the `userName` you provision.

## Google Workspace

Google Workspace provisions only the applications in its own catalog, so it can't send SCIM to
awzy. You can still:

- **Sign in with Google:** create an OAuth client (a web application) in Google Cloud with the
  portal's callback as an authorized redirect URI. Google's issuer is
  `https://accounts.google.com` and its keys are at `https://www.googleapis.com/oauth2/v3/certs`.
  Its `sub` is a numeric ID, so set `subject_claim` to `email`.
- **Provision through another IdP:** if Okta or Entra federates with your Google Workspace,
  provision awzy from it as above.

Without provisioning, a person who signs in is checked with the groups in their token alone, and
awzy-context has no record of them.

## What policies see about a person

Besides their groups and whether they're active, awzy gives policies these attributes of a
person when your IdP sends them over SCIM. Each is absent when it doesn't, so policies test for it
with `has`.

| Attribute | From SCIM | Example |
| --- | --- | --- |
| `title` | `title` | `"Software Engineer"` |
| `employee_type` | `userType`, in lowercase with `_` for spaces | `"contractor"`, `"employee"` |
| `department` | enterprise extension `department` | `"engineering"` |
| `division` | enterprise extension `division` | |
| `organization` | enterprise extension `organization` | |
| `cost_center` | enterprise extension `costCenter` | |
| `manager` | enterprise extension `manager`, as the `Human` it names | `Human::"pat@example.com"` |
| `team` | the group, of those you say are teams, they're directly in | `Group::"sales-emea"` |

Okta's SCIM attribute mappings and Entra's default ones send `title` and `department`. Map your
IdP's employee type to `userType` if it doesn't already, so policies can tell contractors from
employees:

```cedar
forbid (principal is Human, action == Action::"read", resource is Resource)
when {
  principal has employee_type && principal.employee_type == "contractor" &&
  resource.sensitivity == "crown_jewels"
};
```

**Managers.** awzy finds the person a `manager` names by their SCIM ID, the external ID your
IdP keeps for them, or their user name, so it works whether your IdP sends the manager's ID or
their email. A manager who isn't provisioned to awzy is left out. awzy's base policies don't let
managers approve requests on their own: a contractor's manager may work for the same vendor. To
let a person's manager approve their challenged requests, add a policy like this:

```cedar
@id("managers-approve")
permit (principal is Human, action == Action::"approve", resource)
when {
  context has approving_for && context.approving_for has manager &&
  context.approving_for.manager == principal &&
  resource.sensitivity != "crown_jewels"
};
```

**Teams.** Your IdP has groups of every kind, so you say which are teams with the install's
`team_groups` setting: group names, each of which may use `*` (`["sales-*", "security"]`).
Someone in more than one team group is on the smallest. Without the setting nobody has a
`team`, and risk scores compare each person with their smallest group instead.

## Checking it works

1. **Provision one person.** Okta's push, or Entra's **Provision on demand**, sends them right
   away. They appear in the client portal under **Directory** within a few seconds, and
   awzy-context logs `SCIM: user provisioned`.
2. **Push one group** with that person in it. It appears under **Directory** with its members.
3. **Sign in** to the client portal as them. **Look up** shows them with their groups. If they
   show as unknown, the token's subject didn't match their SCIM user name: check
   `subject_claim`.
4. **Deactivate them** in the IdP. awzy-context logs `SCIM: user (de)activated`, and they no
   longer count as provisioned.

## Notifications

awzy finds people in Slack, or emails them, by their email: the primary email your IdP provisions for them, else
their work email, else their user name when it's an address. Provisioning emails, which Okta and
Entra do by default, reaches everyone. See [optional features](/guides/optional-features/).

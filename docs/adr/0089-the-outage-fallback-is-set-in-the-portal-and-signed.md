# 0089. Tool tiers and the outage fallback are set in the client portal and signed

- Status: Accepted
- Date: 2026-10-11
- Amends: [0030](0030-gateway-outage-fallback.md) (where tiers and fail-open tiers live)

## Context

0030 left each MCP tool's sensitivity tier, and which tiers fail open while the PDP is
unreachable, in awzy-gateway's config file, until awzy-context classified tools. awzy-context
doesn't know MCP tools and isn't about to. Meanwhile the tiers decide more than outages: every
check sends the tool's tier to the PDP, and the base policies treat confidential and crown-jewel
tools differently. A config file is edited outside the portal, isn't signed, and leaves no record
of who changed it. 0021 asked for the fallback enforcement points hold to be signed like bundles.

## Decision

- **awzy-policy keeps the setting**, as it keeps enforcement modes: an append-only history of
  versions, each holding tool tiers by `<server>/<tool>`, a default tier per server for its
  unlisted tools, and the tiers that fail open. The latest version is current; with none, every
  tool is unclassified (confidential) and every call fails closed during an outage.
- **Each version is signed** with the bundle key (0021) over `fallback_payload`: its version, who
  set it and when, and the SHA-256 of its canonical text. confidential and crown_jewels can never
  fail open; awzy-policy refuses a setting that says otherwise.
- **awzy-gateway follows it** from awzy-policy and verifies each version with the key PDPs verify
  bundles with. It takes only a newer version that verifies, so a row changed in the database or an
  older setting offered again changes nothing. Until it has one, every tool is unclassified. Its
  config file no longer carries tiers or fail-open tiers, and refuses them.
- **The client portal sets it**, on the Enforcement page, behind the same `switch_enforcement`
  decision as switching the mode, so the base policies ask for the admin's security key. Failing
  open weakens enforcement as surely as monitor mode does.
- 0030's rules stand: only a tool classified by name can fail open, only by replaying a recent
  allow for the same caller, and never for someone contained.

## Consequences

- Tiers change in the portal, take effect in every gateway within seconds, and leave a record of
  who changed them.
- A new install's gateway treats every tool as confidential until an admin classifies them. Demo
  installs seed the synthetic company's tiers from the demo-data Job, under a service account of
  its own that only demo installs have, unless a setting exists.
- Gateways poll awzy-policy every 10 seconds instead of waiting on a change (0077), which is soon
  enough for a setting changed by hand.

# 0121. `audit` sees everything in a portal area and changes nothing

- Status: Accepted
- Date: 2026-10-13
- Amends: [0020](0020-the-portal-is-authorized-by-the-pdp.md) (the portal is authorized by the PDP:
  `view` and `administer` on each `PortalArea`)

## Context

0020 gives each portal area two actions: `view` reads it, `administer` changes what it controls.
For most areas that's enough, since everyone who may read an area sees the same things. Challenges
are different. Everyone has `view` on them, so they can answer the challenges sent to them, and
`view` therefore shows only your own. Listing every challenge and every containment needs
`administer`, which also lets you approve requests and contain or lift principals.

So an external auditor, who must see every challenge and containment and change nothing, can't be
given what they need without the power to act (#326). The External auditors page tells them to
ask the security team for a copy.

## Decision

- **A third action on `PortalArea`, `audit`: see everything in the area, change nothing.** `view`
  keeps meaning what any signed-in person sees there, and `administer` what changes it.
- **Reads that show everyone's records accept `audit` or `administer`**: the challenges list, the
  containments list, a challenge or containment by ID, and the live feeds of both. They ask
  `audit` first, so an auditor's reads aren't logged as denied `administer` checks. Nothing that
  acts accepts `audit`: approving, rejecting, containing, lifting, and resetting a security key
  still need `administer`.
- **The base policy gives the security team `audit` beside `view` and `administer`.** A client
  grants it to auditors with one policy, `permit (principal in Group::"auditors", action in
  [Action::"view", Action::"audit"], resource is PortalArea);`.
- **`viewerCan(action: AUDIT, area: …)`** lets a page show the full records, read-only, to those
  who may audit and not administer.
- **Support sessions stay `view` only.** The base policy that keeps an awzy staff member to `view`
  (0095) forbids `audit` as it forbids every other action.

Rejected: a group the portal knows by name, such as `auditors`. Who may see what is policy (0020),
and a client names its own groups. Rejected: widening `view` on challenges to everyone's records
for some principals. `view` would then mean different things in different areas, and a policy
that grants `view` on every area, as the auditors page suggests today, would quietly start showing
everyone's challenges to whoever it names.

## Consequences

- An auditor can be given every challenge and containment with no way to change them, and the
  External auditors page grants `audit` instead of describing a workaround.
- Areas whose `view` already shows everything don't need `audit` yet. A future area with personal
  records, like challenges, uses `audit` for "everyone's" rather than a new action.
- A client policy that grants `administer` without `audit` keeps working, since every read that
  accepts `audit` also accepts `administer`.

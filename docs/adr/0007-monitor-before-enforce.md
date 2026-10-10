# 0007. Monitor mode before enforce

- Status: Accepted, amended by [0020](0020-the-portal-is-authorized-by-the-pdp.md) (the portal's own access is always enforced)
- Date: 2026-10-08

## Context

No client will let a new product deny live traffic on day one, and automated containments act on
people based on model output.

## Decision

- Every enforcement switch has two modes. In monitor, the PDP decides and records the verdict but
  the PEP always allows, and challenges are logged as "would challenge" and not sent. In enforce,
  the PEP applies the verdict.
- The mode is set separately per install, per policy set, and for automated containments. Every
  new one starts in monitor.
- The client portal shows would-deny and would-contain rates with examples, and a client admin
  switches each scope to enforce after review.
- Automated containments stay in monitor until the share lifted on human review meets a target
  set from design-partner data (Phase 4).

## Consequences

- The client portal's first pages are decision search, explain-decision, and the mode switch.
- Monitor mode is how a design partner sees value before awzy can block anything.

# 0037. Changes reach `main` only through pull requests, made in worktrees

- Status: Accepted
- Date: 2026-10-09

## Context

Several sessions, human and Claude, have been committing straight to `main` from one checkout.
CI on `main` kept cancelling or failing on each other's pushes, and on 2026-10-09 a
`git pull --rebase --autostash` in the shared checkout stashed one session's uncommitted work and
re-applied it with merge conflicts. With 0035, every merge to `main` is also a production deploy
of the corp portal and registry.

## Decision

- Every change goes on a branch and reaches `main` through a pull request whose CI passes.
- Each session works in its own git worktree (`git worktree add ../awzy-<topic> -b <topic>
  origin/main`), never in another session's checkout, and never pulls, stashes, or resets a
  checkout it doesn't own.
- Commits stage explicit paths, never everything.

## Consequences

- Not enforced yet: branch protection and rulesets on a private repository need GitHub Pro (or
  a public repository). Until then this is a rule, not a guarantee. Once available: PRs
  required (admins included), the `bazel` check required, no force pushes, squash merges.
- No required approvals while there is one maintainer: GitHub doesn't let authors approve their
  own PRs.
- Branches that touch the same file (for example `.github/workflows/ci.yml`) need a rebase when
  the other merges first.

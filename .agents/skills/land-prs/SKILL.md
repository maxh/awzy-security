---
name: land-prs
description: Get open pull requests merged: rebase the ones main has moved past, approve held CI runs, read and fix failures, and keep auto-merge on. Use when asked to get PRs merged or passing, and after opening several.
---

# Landing pull requests

Loop until every PR is merged. Check every minute or two; a PR that sits conflicted or failing
for a quarter of an hour is time lost. Don't wait idle on one PR: sweep them all each pass.

## Each pass, for each open PR

```sh
gh pr list --author @me --state open --json number,headRefName,mergeStateStatus
```

1. **Held runs.** Pushes from the fix-prs workflow start runs that wait for approval
   (`action_required`). Approve them:
   ```sh
   for id in $(gh run list --branch <branch> --json databaseId,conclusion \
       -q '.[] | select(.conclusion=="action_required") | .databaseId'); do
     gh api -X POST repos/{owner}/{repo}/actions/runs/$id/approve; done
   ```
2. **Conflicts (`DIRTY`).** Rebase on main and force-push:
   `git fetch origin && git rebase origin/main`. For a conflict in `Cargo.lock` or
   `cargo-bazel-lock.json`, take main's (`git checkout --ours <file>` during a rebase) and let
   fix-prs re-pin. A generated schema is the same: take either and let fix-prs regenerate it.
   Anything else is a real conflict to resolve by hand.
3. **Stacked PRs.** When a PR's base branch is squash-merged, GitHub retargets it to main but its
   branch still carries the base's commits. Replay only its own:
   `git rebase --onto origin/main <last commit of the old base>`.
4. **Failures.** Read the failed job's log rather than guessing:
   ```sh
   gh api repos/{owner}/{repo}/actions/jobs/<job id>/logs | grep -E '##\[error\]|error(\[E[0-9]+\])?:|FAIL|panicked'
   ```
   (`<job id>` is the end of the check's `detailsUrl` in `gh pr view <n> --json statusCheckRollup`.)
   - `Digests do not match`: a stale lockfile. fix-prs re-pins it; wait for its push.
   - Formatting: fix-prs formats changed files; wait for its push.
   - Anything else (a compile error, clippy, a test, a ratchet): fix it, push, and move on to the
     next PR while CI runs.
5. **Auto-merge.** Keep it on: `gh pr merge <n> --auto --squash`. A PR green and `CLEAN` without
   it is stuck for no reason.

## Avoiding the rebases

- Keep PRs independent of each other where you can: two PRs editing the same lines rebase each
  other every time one lands.
- Land the PR the others depend on first, and rebase the rest once, after it.
- Pull before pushing again: fix-prs may have pushed to the branch.

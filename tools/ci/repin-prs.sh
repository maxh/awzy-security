#!/usr/bin/env bash
# Keeps open pull requests mergeable when the only thing in the way is crate_universe's lockfiles
# (Cargo.lock, cargo-bazel-lock.json, MODULE.bazel.lock). cargo-bazel-lock.json records a checksum
# of every Cargo.toml, so any two PRs that touch a crate's dependencies conflict there, however
# unrelated they are.
#
# For each open PR from this repository that conflicts with main: merge main in with the lockfile
# merge driver (tools/git-merge-lockfile). If that was the only conflict, re-pin, commit, push to
# the PR's branch, and start its CI (pushes made with the workflow's token don't). If anything
# else conflicts, leave the branch alone and say so on the PR, once.
#
# Run by .github/workflows/repin-prs.yml after each push to main, with GH_TOKEN set.
set -euo pipefail

repo=${GITHUB_REPOSITORY:?}
git config user.name "awzy repin"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git config merge.awzy-lockfile.name "re-pin crate_universe lockfiles instead of merging them"
git config merge.awzy-lockfile.driver "tools/git-merge-lockfile %O %A %B %P"
marker="$(git rev-parse --git-dir)/awzy-repin-needed"

# GitHub computes mergeability lazily after main moves; ask until it has an answer.
mergeable() {
  local state
  for _ in 1 2 3 4 5 6; do
    state=$(gh pr view "$1" -R "$repo" --json mergeable -q .mergeable)
    [ "$state" != UNKNOWN ] && { echo "$state"; return; }
    sleep 10
  done
  echo UNKNOWN
}

prs=$(gh pr list -R "$repo" --state open --base main --json number,headRefName,isCrossRepository \
  -q '.[] | select(.isCrossRepository | not) | "\(.number) \(.headRefName)"')
[ -n "$prs" ] || { echo "no open pull requests"; exit 0; }

while read -r number branch; do
  [ "$(mergeable "$number")" = CONFLICTING ] || continue
  echo "::group::#$number ($branch) conflicts with main"
  git fetch -q origin "$branch" main
  git checkout -q -B "$branch" "origin/$branch"
  rm -f "$marker"
  if git merge -q --no-edit origin/main; then
    if [ -f "$marker" ]; then
      CARGO_BAZEL_REPIN=1 bazel query --noshow_progress --ui_event_filters=-info,-warning //... >/dev/null
      # Again without it, so MODULE.bazel.lock doesn't record CARGO_BAZEL_REPIN=1.
      bazel query --noshow_progress --ui_event_filters=-info,-warning //... >/dev/null
      git add Cargo.lock cargo-bazel-lock.json MODULE.bazel.lock
      git diff --cached --quiet || git commit -q -m "Re-pin crates after merging main"
    fi
    git push -q origin "HEAD:$branch"
    gh workflow run ci.yml -R "$repo" --ref "$branch"
    echo "merged main into #$number and re-pinned; CI started"
  else
    files=$(git diff --name-only --diff-filter=U | sed 's/^/`/; s/$/`/' | paste -sd, - | sed 's/,/, /g')
    git merge --abort
    note="<!-- awzy-repin -->"
    if ! gh pr view "$number" -R "$repo" --json comments -q '.comments[].body' | grep -qF "$note"; then
      gh pr comment "$number" -R "$repo" --body "$note
This branch conflicts with \`main\` in $files, beyond the lockfiles, so it needs a rebase by hand. Lockfile conflicts are re-pinned automatically (tools/git-hooks, \`.github/workflows/repin-prs.yml\`)."
    fi
    echo "#$number needs a hand rebase: $files"
  fi
  echo "::endgroup::"
done <<< "$prs"

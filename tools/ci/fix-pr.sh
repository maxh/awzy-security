#!/usr/bin/env bash
# Makes a pull request's mechanical fixes so nobody has to run Bazel locally for them: re-pins
# crate_universe's lockfiles, regenerates both portals' GraphQL schemas, and formats the Rust
# (rustfmt) and TypeScript (oxfmt) the PR changed. Relay's output isn't committed, so it isn't
# regenerated here (ADR 0090). Each step runs only when the PR touches what it fixes. When anything
# changed, it pushes one commit to the PR's branch and starts the PR's CI, which the workflow
# token's push doesn't.
#
# Lint findings aren't fixed here: clippy's and oxlint's fixes can change behavior, so CI
# reports them instead.
#
# Run by .github/workflows/fix-prs.yml on pull requests from this repository, with GH_TOKEN and
# BRANCH set, on a full checkout of the PR's branch.
set -euo pipefail
repo=${GITHUB_REPOSITORY:?} branch=${BRANCH:?}
cd "$(git rev-parse --show-toplevel)"

# What the PR changes, against where it left main.
base=$(git merge-base origin/main HEAD)
changed=$(git diff --name-only --diff-filter=ACMR "$base" HEAD)
touches() { grep -qE "$1" <<<"$changed"; }
fixed=()
paths=() # what the fixes may change; only these are committed

if touches '(^|/)Cargo\.toml$|^Cargo\.lock$|^MODULE\.bazel$'; then
  echo "::group::Re-pin crates"
  CARGO_BAZEL_REPIN=1 bazel query --noshow_progress --ui_event_filters=-info,-warning //... >/dev/null
  # Again without it, so MODULE.bazel.lock doesn't record CARGO_BAZEL_REPIN=1.
  bazel query --noshow_progress --ui_event_filters=-info,-warning //... >/dev/null
  git diff --quiet -- Cargo.lock MODULE.bazel.lock || fixed+=("re-pin crates")
  paths+=(Cargo.lock MODULE.bazel.lock)
  echo "::endgroup::"
fi

if touches '^libs/(model|portal)/|^portals/[^/]+/backend/|^portals/[^/]+/frontend/schema\.graphql$'; then
  echo "::group::Regenerate GraphQL schemas"
  for portal in client corp; do
    bazel run --config=ci "//portals/$portal/frontend:update_schema" </dev/null
  done
  [ -z "$(git status --porcelain -- 'portals/*/frontend/schema.graphql')" ] || fixed+=("regenerate schemas")
  paths+=('portals/*/frontend/schema.graphql')
  echo "::endgroup::"
fi

mapfile -t rust_files < <(grep -E '\.rs$' <<<"$changed" || true)
if [ ${#rust_files[@]} -gt 0 ]; then
  echo "::group::rustfmt"
  bazel run --config=ci --noshow_progress --ui_event_filters=-info \
    @rules_rust//tools/upstream_wrapper:rustfmt -- --config-path "$PWD/rustfmt.toml" "${rust_files[@]}"
  git diff --quiet -- "${rust_files[@]}" || fixed+=("rustfmt")
  paths+=("${rust_files[@]}")
  echo "::endgroup::"
fi

mapfile -t ts_files < <(grep -E '^portals/.*\.(ts|tsx|js|mjs)$' <<<"$changed" | grep -v '/__generated__/' || true)
if [ ${#ts_files[@]} -gt 0 ]; then
  echo "::group::oxfmt"
  [ -x node_modules/.bin/oxfmt ] || pnpm install --frozen-lockfile
  node_modules/.bin/oxfmt "${ts_files[@]}"
  git diff --quiet -- "${ts_files[@]}" || fixed+=("oxfmt")
  paths+=("${ts_files[@]}")
  echo "::endgroup::"
fi

if [ ${#fixed[@]} -eq 0 ]; then
  echo "nothing to fix"
  exit 0
fi

summary=$(IFS=,; echo "${fixed[*]}" | sed 's/,/, /g')
git status --short
git config user.name "awzy fix"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git add -A -- "${paths[@]}"
git commit -q -m "fix: $summary"
# A push that lost a race with a newer one from the author is fine: that push starts its own run.
if ! git push -q origin "HEAD:$branch"; then
  echo "the branch moved while fixing; the run for its newer commit fixes that one"
  exit 0
fi
gh workflow run ci.yml -R "$repo" --ref "$branch"
echo "pushed \"fix: $summary\" to $branch; CI started"

#!/usr/bin/env bash
# Regenerates both portals' GraphQL schemas (from the Rust backends) and their Relay artifacts,
# and pushes them to the pull request's branch when they changed, so nobody has to run Bazel
# locally after changing a resolver. Then starts the PR's CI, which the workflow token's push
# doesn't.
#
# Run by .github/workflows/schema-prs.yml on pull requests from this repository, with GH_TOKEN
# and BRANCH set, on a checkout of the PR's branch.
set -euo pipefail
repo=${GITHUB_REPOSITORY:?} branch=${BRANCH:?}

for portal in client corp; do
  bazel run --config=ci "//portals/$portal/frontend:update_schema" </dev/null
done
pnpm install --frozen-lockfile
for portal in client corp; do
  (cd "portals/$portal/frontend" && pnpm relay)
done

changed=$(git status --porcelain -- 'portals/*/frontend/schema.graphql' 'portals/*/frontend/src/__generated__')
if [ -z "$changed" ]; then
  echo "schemas and Relay artifacts are current"
  exit 0
fi
echo "$changed"
git config user.name "awzy schema"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git add -- portals/*/frontend/schema.graphql portals/*/frontend/src/__generated__
git commit -q -m "Regenerate GraphQL schemas and Relay artifacts"
git push -q origin "HEAD:$branch"
gh workflow run ci.yml -R "$repo" --ref "$branch"
echo "pushed regenerated schemas to $branch; CI started"

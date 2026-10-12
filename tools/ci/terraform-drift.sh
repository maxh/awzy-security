#!/usr/bin/env bash
# terraform-drift.sh <module>...: plans each corp Terraform module (deploy/terraform/<module>)
# against its recorded state, without refreshing or locking, and says which have merged changes
# that aren't applied: the terraform-corp workflow's apply is waiting for approval, or failed.
#
# Keeps one GitHub issue open per module while it differs, listing what would change, and closes
# it once the module is applied. Prints only resource addresses: plans hold account IDs and ARNs,
# and these logs are public.
#
# Needs DEPLOYMENT_ENV (the corp deployment.env, from a repository secret), AWS credentials for
# the build account in the environment, and GH_TOKEN.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# shellcheck source=tools/ci/terraform-env.sh
source tools/ci/terraform-env.sh

title() { echo "Terraform not applied: deploy/terraform/$1"; }

open_issue() { # <module> <changes>
  local t number body
  t=$(title "$1")
  number=$(gh issue list --state open --search "in:title \"$t\"" --json number,title --jq ".[] | select(.title == \"$t\") | .number" | head -1)
  body=$(printf 'Merged Terraform in `deploy/terraform/%s` differs from what was last applied (checked at %s):\n\n```\n%s\n```\n\nThe `terraform-corp` workflow applies it once its run for the merge is approved (`deploy/terraform/corp/README.md`). This issue closes itself once the module is applied.\n' \
    "$1" "${GITHUB_SHA:0:7}" "$2")
  if [ -n "$number" ]; then
    gh issue edit "$number" --body "$body" >/dev/null
  else
    gh issue create --title "$t" --body "$body" >/dev/null
  fi
}

close_issue() { # <module>
  local t number
  t=$(title "$1")
  number=$(gh issue list --state open --search "in:title \"$t\"" --json number,title --jq ".[] | select(.title == \"$t\") | .number" | head -1)
  [ -z "$number" ] || gh issue close "$number" --comment "Applied as of ${GITHUB_SHA:0:7}." >/dev/null
}

status=0
for module in "$@"; do
  dir=deploy/terraform/$module
  if ! (cd "$dir" && tofu init -input=false >/dev/null 2>init.err); then
    echo "$module: init failed"
    redact < "$dir/init.err" | tail -20
    status=1
    continue
  fi
  set +e
  (cd "$dir" && tofu plan -input=false -lock=false -refresh=false -detailed-exitcode -out=drift.plan >/dev/null 2>plan.err)
  code=$?
  set -e
  case $code in
    0)
      echo "$module: everything merged is applied"
      close_issue "$module"
      ;;
    2)
      changes=$(plan_changes "$dir" drift.plan)
      echo "$module: merged but not applied:"
      echo "$changes"
      open_issue "$module" "$changes"
      ;;
    *)
      echo "$module: plan failed"
      redact < "$dir/plan.err" | tail -20
      status=1
      ;;
  esac
done
exit $status

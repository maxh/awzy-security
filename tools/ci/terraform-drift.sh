#!/usr/bin/env bash
# terraform-drift.sh <module>...: plans each corp Terraform module (deploy/terraform/<module>)
# against its recorded state, without refreshing or locking, and says which have merged changes
# that nobody has applied. CI never applies Terraform; a person does (deploy/terraform/corp/README.md).
#
# Keeps one GitHub issue open per module while it differs, listing what would change, and closes
# it once the module is applied. Prints only resource addresses: plans hold account IDs and ARNs,
# and these logs are public.
#
# Needs DEPLOYMENT_ENV (the corp deployment.env, from a repository secret), AWS credentials for
# the build account in the environment, and GH_TOKEN.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

env_file=deploy/terraform/corp/deployment.env
printf '%s\n' "${DEPLOYMENT_ENV:?DEPLOYMENT_ENV is not set}" > "$env_file"
set -a
# shellcheck disable=SC1090
source "$env_file"
set +a

# The modules name the deployment's AWS profiles; here those are this job's credentials.
mkdir -p ~/.aws
for profile in "${TF_VAR_build_profile:-}" "${TF_VAR_management_profile:-}"; do
  [ -n "$profile" ] || continue
  printf '[%s]\naws_access_key_id = %s\naws_secret_access_key = %s\naws_session_token = %s\n' \
    "$profile" "$AWS_ACCESS_KEY_ID" "$AWS_SECRET_ACCESS_KEY" "$AWS_SESSION_TOKEN" >> ~/.aws/credentials
done
# Without a refresh nothing is read from Cloudflare; its provider only needs a token to start.
cf_token=$(mktemp)
printf '%040d' 0 > "$cf_token"
export TF_VAR_cloudflare_token_file="$cf_token"

# Twelve-digit numbers are account IDs; never print one.
redact() { sed -E 's/[0-9]{12}/<account>/g'; }

title() { echo "Terraform not applied: deploy/terraform/$1"; }

open_issue() { # <module> <changes>
  local t number body
  t=$(title "$1")
  number=$(gh issue list --state open --search "in:title \"$t\"" --json number,title --jq ".[] | select(.title == \"$t\") | .number" | head -1)
  body=$(printf 'Merged Terraform in `deploy/terraform/%s` differs from what was last applied (checked at %s):\n\n```\n%s\n```\n\nApply it as `deploy/terraform/corp/README.md` says: CI never applies Terraform. This issue closes itself once the module is applied.\n' \
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
      changes=$(cd "$dir" && tofu show -json drift.plan |
        jq -r '.resource_changes[] | select(.change.actions != ["no-op"] and .change.actions != ["read"]) | "\(.change.actions | join("+"))\t\(.address)"' |
        redact)
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

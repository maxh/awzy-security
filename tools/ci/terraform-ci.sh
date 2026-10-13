#!/usr/bin/env bash
# terraform-ci.sh <deployment> plan|apply: plans a deployment's Terraform modules, in order, and
# with `apply` applies what it planned (ADR 0115). A deployment is the GitHub environment that
# applies it: corp-terraform (the build account's corp modules), example-aws (the example
# client's install account), or example-gcp (the example client's Google Cloud project).
#
# `plan`, for a pull request: against the recorded state without refreshing or locking, as the
# read-only plan roles, and with PR_NUMBER set keeps one comment on the pull request listing the
# changes. `apply`, after merge: refreshes, plans, and applies that plan, as the apply roles.
#
# Prints resource addresses only, never plan details: plans hold account IDs, and these logs and
# comments are public. Needs DEPLOYMENT_ENV, the cloud's credentials (AWS's, see terraform-env.sh;
# Google Cloud's, from google-github-actions/auth), GH_TOKEN, and for corp's `apply`
# CLOUDFLARE_TOKEN.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

usage="usage: terraform-ci.sh corp-terraform|example-aws|example-gcp plan|apply"
deployment=${1:?$usage}
mode=${2:?$usage}
case $mode in plan | apply) ;; *) echo "$usage" >&2; exit 2 ;; esac
case $deployment in
  corp-terraform)
    env_dir=deploy/terraform/corp
    title="Corp Terraform"
    modules=(corp/build corp/corp-portal)
    ;;
  example-aws)
    env_dir=deploy/terraform/examples/example-client
    title="The example client's Terraform (AWS)"
    # In order: each reads the outputs of those before it.
    modules=(examples/example-client/install examples/example-client/monitoring examples/example-client/grafana examples/example-client/bootstrap)
    ;;
  example-gcp)
    env_dir=deploy/terraform/examples/example-client-gcp
    title="The example client's Terraform (Google Cloud)"
    modules=(examples/example-client-gcp/install examples/example-client-gcp/bootstrap examples/example-client-gcp/monitoring)
    ;;
  *) echo "$usage" >&2; exit 2 ;;
esac

# shellcheck source=tools/ci/terraform-env.sh
source tools/ci/terraform-env.sh

status=0
report=""
for module in "${modules[@]}"; do
  dir=deploy/terraform/$module
  if ! (cd "$dir" && tofu init -input=false >/dev/null 2>init.err); then
    echo "$module: init failed"
    redact < "$dir/init.err" | tail -20
    report+=$'\n'"**$module**: init failed; see the job's log."$'\n'
    status=1
    continue
  fi
  flags=(-input=false -detailed-exitcode -out=ci.plan)
  [ "$mode" = plan ] && flags+=(-lock=false -refresh=false)
  set +e
  (cd "$dir" && tofu plan "${flags[@]}" >/dev/null 2>plan.err)
  code=$?
  set -e
  case $code in
    0)
      echo "$module: no changes"
      report+=$'\n'"**$module**: no changes."$'\n'
      ;;
    2)
      changes=$(plan_changes "$dir" ci.plan)
      echo "$module: changes:"
      echo "$changes"
      report+=$'\n'"**$module**:"$'\n\n```\n'"$changes"$'\n```\n'
      if [ "$mode" = apply ]; then
        if (cd "$dir" && tofu apply -input=false ci.plan >/dev/null 2>apply.err); then
          echo "$module: applied"
        else
          echo "$module: apply failed"
          redact < "$dir/apply.err" | tail -20
          status=1
          # A later module may read this one's outputs: stop here.
          break
        fi
      fi
      ;;
    *)
      echo "$module: plan failed"
      redact < "$dir/plan.err" | tail -20
      report+=$'\n'"**$module**: plan failed; see the job's log."$'\n'
      status=1
      ;;
  esac
done

if [ "$mode" = plan ] && [ -n "${PR_NUMBER:-}" ]; then
  marker="<!-- terraform-ci $deployment -->"
  body=$(printf '%s\n### %s\n\nWhat merging this changes, planned against the recorded state (without a refresh). The `terraform` workflow applies it after merge, once someone approves the `%s` environment.\n%s' "$marker" "$title" "$deployment" "$report")
  existing=$(gh api "repos/{owner}/{repo}/issues/$PR_NUMBER/comments" --paginate \
    --jq ".[] | select(.body | startswith(\"$marker\")) | .id" | head -1)
  if [ -n "$existing" ]; then
    gh api -X PATCH "repos/{owner}/{repo}/issues/comments/$existing" -f body="$body" >/dev/null
  else
    gh pr comment "$PR_NUMBER" --body "$body" >/dev/null
  fi
fi
[ -z "${GITHUB_STEP_SUMMARY:-}" ] || printf '%s\n' "$report" >> "$GITHUB_STEP_SUMMARY"
exit $status

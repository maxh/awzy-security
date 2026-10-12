#!/usr/bin/env bash
# terraform-corp.sh plan|apply <module>...: plans each corp Terraform module (deploy/terraform/<module>),
# in order, and with `apply` applies what it planned (ADR 0115).
#
# `plan`, for a pull request: against the recorded state without refreshing or locking, as the
# read-only plan role, and with PR_NUMBER set keeps one comment on the pull request listing the
# changes. `apply`, after merge: refreshes, plans, and applies that plan, as the apply role.
#
# Prints resource addresses only, never plan details: plans hold account IDs, and these logs and
# comments are public. Needs DEPLOYMENT_ENV, AWS credentials for the build account, GH_TOKEN, and
# for `apply` CLOUDFLARE_TOKEN.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

mode=${1:?usage: terraform-corp.sh plan|apply <module>...}
shift
case $mode in plan | apply) ;; *) echo "usage: terraform-corp.sh plan|apply <module>..." >&2; exit 2 ;; esac

# shellcheck source=tools/ci/terraform-env.sh
source tools/ci/terraform-env.sh

status=0
report=""
for module in "$@"; do
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
  marker='<!-- terraform-corp -->'
  body=$(printf '%s\n### Corp Terraform\n\nWhat merging this changes, planned against the recorded state (without a refresh). The `terraform-corp` workflow applies it after merge, once someone approves the `corp-terraform` environment.\n%s' "$marker" "$report")
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

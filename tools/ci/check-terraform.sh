#!/usr/bin/env bash
# Every Terraform directory is formatted and valid, so a change to a shared module that breaks a
# caller fails its PR, not the next install's apply (bootstrap's stale depends_on after #157).
# Needs no credentials or state: `init -backend=false` only fetches providers.
#
# A root module's committed .terraform.lock.hcl must already match its providers (`init` there is
# read-only). Shared modules commit no lock file; the one `init` writes is removed again.
#
#   tools/ci/check-terraform.sh     (CI's chart job; TF_PLUGIN_CACHE_DIR speeds up repeat runs)
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
tofu fmt -check -recursive deploy/terraform
status=0
for dir in $(git ls-files 'deploy/terraform/*.tf' | xargs -n1 dirname | sort -u); do
  lock=readonly
  git ls-files --error-unmatch "$dir/.terraform.lock.hcl" >/dev/null 2>&1 || lock=update
  if ! out=$(cd "$dir" && tofu init -backend=false -lockfile=$lock -input=false -no-color 2>&1 && tofu validate -no-color 2>&1); then
    echo "::error::$dir is not valid (a stale lock file: run \`tofu init\` there and commit it):" >&2
    echo "$out" | grep -v '^\s*$' | tail -8 >&2
    status=1
  else
    echo "$dir: valid"
  fi
  [ $lock = update ] && rm -f "$dir/.terraform.lock.hcl"
done
exit $status

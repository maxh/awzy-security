#!/usr/bin/env bash
# Fails when something that identifies a real deployment, account, or person is in the
# repository (keep-deployments-out skill): AWS account IDs and ARNs, sign-in portal URLs, real
# install QIDs, awzy email addresses. Placeholders are in identifier-allowlist.txt.
#
#   tools/ci/check-identifiers.sh            # every tracked file
#   tools/ci/check-identifiers.sh --staged   # lines staged for commit (the pre-commit hook)
#   tools/ci/check-identifiers.sh --text     # stdin: a PR's title and description (CI)
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

pattern='\b[0-9]{12}\b|ssoins-[0-9a-f]{8,}|[a-z0-9-]+\.awsapps\.com|install_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|[A-Za-z0-9._%+-]+@awzysecurity\.com'
allow=$(grep -vE '^\s*(#|$)' tools/ci/identifier-allowlist.txt | paste -sd'|' -)
# Generated or binary-ish files whose long numbers are hashes and coordinates, not identifiers.
skip=(':!*.lock' ':!*pnpm-lock.yaml' ':!*.svg' ':!*.png' ':!*/__generated__/*' ':!tools/ci/identifier-allowlist.txt')

# Reads "where<TAB>line" pairs; prints the ones holding a non-placeholder identifier.
findings() {
  local where line hit status=0
  while IFS=$'\t' read -r where line; do
    while read -r hit; do
      [ -z "$hit" ] && continue
      echo "$hit" | grep -qE "$allow" && continue
      echo "$where: $hit"
      status=1
    done < <(printf '%s\n' "$line" | grep -oE "$pattern" || true)
  done
  return $status
}

case "${1:-}" in
  --staged)
    input=$(git diff --cached -U0 --diff-filter=ACMR -- . "${skip[@]}" |
      awk '/^\+\+\+ b\//{f=substr($0,7)} /^\+[^+]/{print f"\t"substr($0,2)}') ;;
  --text)
    input=$(awk '{print "PR text\t"$0}') ;;
  "")
    input=$(git grep -nE "$pattern" -- . "${skip[@]}" | sed -E 's/^([^:]+:[0-9]+):/\1\t/' || true) ;;
  *) echo "usage: $0 [--staged | --text]" >&2; exit 2 ;;
esac

if ! out=$(printf '%s\n' "$input" | findings); then
  echo "$out" >&2
  echo "These look like real account, resource, or person identifiers, which don't belong in this public repository (.agents/skills/keep-deployments-out). Use a placeholder, or add a true placeholder to tools/ci/identifier-allowlist.txt." >&2
  exit 1
fi
echo "no identifiers found"

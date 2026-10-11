#!/usr/bin/env bash
# Fails when something that identifies a real deployment, account, or person is in the
# repository (keep-deployments-out skill): AWS account IDs and ARNs, sign-in portal URLs, real
# install QIDs, awzy email addresses. Placeholders are in identifier-allowlist.txt. Binary files,
# which it can't search, fail unless they're a kind it allows (binary_allowed).
#
#   tools/ci/check-identifiers.sh            # every tracked file
#   tools/ci/check-identifiers.sh --staged   # lines staged for commit (the pre-commit hook)
#   tools/ci/check-identifiers.sh --text     # stdin: a PR's title and description (CI)
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# A cheap first pass over files; hits() decides what's really an identifier.
prefilter='[0-9]{12}|ssoins-|awsapps\.com|install_[0-9a-f]{8}-|@awzysecurity\.com'
uuid='[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}'
allow=$(grep -vE '^\s*(#|$)' tools/ci/identifier-allowlist.txt | paste -sd'|' -)
# Generated or binary-ish files whose long numbers are hashes and coordinates, not identifiers.
skip=(':!*.lock' ':!cargo-bazel-lock.json' ':!*pnpm-lock.yaml' ':!*.svg' ':!*.png' ':!*/__generated__/*' ':!tools/ci/identifier-allowlist.txt')

# The identifiers in one line. Account IDs are standalone 12-digit tokens, looked for after
# removing UUIDs, whose last group is 12 digits too. No \b: GNU and BSD grep disagree on it.
hits() {
  printf '%s\n' "$1" | grep -oE "install_$uuid|ssoins-[0-9a-f]{8,}|[a-z0-9-]+\.awsapps\.com|[A-Za-z0-9._%+-]+@awzysecurity\.com" || true
  printf '%s\n' "$1" | sed -E "s/$uuid//g" | grep -oE '[[:alnum:]]+' | grep -xE '[0-9]{12}' || true
}

# Reads "where<TAB>line" pairs; prints the ones holding a non-placeholder identifier.
findings() {
  local where line hit status=0
  while IFS=$'\t' read -r where line; do
    while read -r hit; do
      [ -z "$hit" ] && continue
      echo "$hit" | grep -qE "$allow" && continue
      echo "$where: $hit"
      status=1
    done < <(hits "$line")
  done
  return $status
}

# Binary files can't be searched (git grep skips them), so only these kinds may be committed: a
# saved Terraform plan, a zip of state with every account ID in it, got in that way (#166).
binary_allowed='\.(png|jpe?g|gif|ico|webp|woff2?)$|^docs/beyond-zero\.pdf$'

# The binary files among `git ls-files`/staged paths on stdin ("path" per line).
unreadable() {
  local path
  while read -r path; do
    if [ -n "$path" ] && ! echo "$path" | grep -qE "$binary_allowed"; then echo "$path"; fi
  done
}

case "${1:-}" in
  --staged)
    # numstat prints "-<TAB>-" for a binary file.
    binaries=$(git diff --cached --numstat --diff-filter=ACMR | awk -F'\t' '$1=="-" && $2=="-" {print $3}' | unreadable) ;;
  "")
    # Regular files (not symlinks) that git grep -I doesn't treat as text, and aren't empty.
    binaries=$(comm -23 <(git ls-files -s | awk '$1 != "120000" {print $4}' | sort) <(git grep -I -l '' | sort) |
      while read -r f; do if [ -s "$f" ]; then echo "$f"; fi; done | unreadable) ;;
  *) binaries="" ;;
esac
if [ -n "$binaries" ]; then
  printf '%s\n' "$binaries" | sed 's/$/: a binary file, which this check can'"'"'t search for identifiers/' >&2
  echo "Commit text instead, or, for an image or font, a kind tools/ci/check-identifiers.sh allows (binary_allowed)." >&2
  exit 1
fi

case "${1:-}" in
  --staged)
    input=$(git diff --cached -U0 --diff-filter=ACMR -- . "${skip[@]}" |
      awk '/^\+\+\+ b\//{f=substr($0,7)} /^\+[^+]/{print f"\t"substr($0,2)}') ;;
  --text)
    input=$(awk '{print "PR text\t"$0}') ;;
  "")
    input=$(git grep -nE "$prefilter" -- . "${skip[@]}" | sed -E 's/^([^:]+:[0-9]+):/\1\t/' || true) ;;
  *) echo "usage: $0 [--staged | --text]" >&2; exit 2 ;;
esac

if ! out=$(printf '%s\n' "$input" | findings); then
  echo "$out" >&2
  echo "These look like real account, resource, or person identifiers, which don't belong in this public repository (.agents/skills/keep-deployments-out). Use a placeholder, or add a true placeholder to tools/ci/identifier-allowlist.txt." >&2
  exit 1
fi
echo "no identifiers found"

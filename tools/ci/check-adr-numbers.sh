#!/usr/bin/env bash
# Fails when two ADRs share a number, or an ADR's heading doesn't carry its file's number. Two pull
# requests that each take "the next number" both pass on their own; this catches the second one
# after the first lands, when the branch is rebased.
#
#   tools/ci/check-adr-numbers.sh
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

status=0
dupes=$(git ls-files 'docs/adr/[0-9]*.md' | sed -E 's|docs/adr/([0-9]+)-.*|\1|' | sort | uniq -d)
for n in $dupes; do
  echo "::error::ADR $n is taken by more than one file; renumber the newer one to the next free number:" >&2
  git ls-files "docs/adr/$n-*.md" | sed 's/^/  /' >&2
  status=1
done
for f in $(git ls-files 'docs/adr/[0-9]*.md'); do
  n=$(basename "$f" | cut -d- -f1)
  if ! head -1 "$f" | grep -q "^# $n\. "; then
    echo "::error file=$f::its heading should start with \"# $n. \"" >&2
    status=1
  fi
done
exit $status

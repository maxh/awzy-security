#!/usr/bin/env bash
# Fails when a new Postgres migration would break the version still running beside it during a
# rollout (ADR 0072). Migrations expand: they add tables, columns that are nullable or have a
# default, and indexes. Dropping, renaming, retyping, or adding a NOT NULL column without a default
# takes a release that stops using the thing first; that migration marks the line
#   -- awzy: contract (stopped using in <version>)
#
#   tools/ci/check-migrations.sh            # migrations that aren't on origin/main yet
#   tools/ci/check-migrations.sh FILE...    # these files
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

if [ $# -gt 0 ]; then
  files=("$@")
else
  git fetch --quiet --no-tags --depth=1 origin main 2>/dev/null || true
  base=$(git ls-tree -r --name-only origin/main 2>/dev/null || true)
  files=()
  while IFS= read -r f; do
    grep -qxF "$f" <<<"$base" || files+=("$f")
  done < <(git ls-files '*/migrations/*.sql')
fi

# What a running older version can't survive. Case-insensitive, one statement per line as the
# migrations are written; CREATE TABLE's own NOT NULL columns are fine, since nothing reads a new
# table yet.
if [ "${#files[@]}" -eq 0 ]; then
  echo "No new migrations."
  exit 0
fi

breaking='drop[[:space:]]+(table|column)|rename|alter[[:space:]]+column.*([[:space:]]type[[:space:]]|set[[:space:]]+not[[:space:]]+null)'
failed=0
for f in "${files[@]}"; do
  n=0
  while IFS= read -r line; do
    n=$((n + 1))
    [[ "$line" =~ awzy:\ contract ]] && continue
    code=$(printf '%s' "${line%%--*}" | tr '[:upper:]' '[:lower:]')
    why=""
    if grep -qE "$breaking" <<<"$code"; then
      why="drops, renames, or retypes something the running version may still use"
    elif grep -qE 'add[[:space:]]+(column[[:space:]]+)?' <<<"$code" && grep -q 'not[[:space:]]*null' <<<"$code" && ! grep -q 'default' <<<"$code"; then
      why="adds a NOT NULL column without a default, which the running version's inserts don't set"
    fi
    if [ -n "$why" ]; then
      echo "::error file=$f,line=$n::$why (ADR 0072). Expand now and contract in a later release, marking that line '-- awzy: contract (stopped using in <version>)'."
      echo "$f:$n: $line"
      failed=1
    fi
  done <"$f"
done
exit "$failed"

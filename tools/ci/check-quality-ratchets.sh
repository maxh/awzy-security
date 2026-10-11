#!/usr/bin/env bash
# Counts patterns the code-quality pass is removing (#221 to #227) and fails if any is higher here
# than where this branch meets main, or if a file grows past the size limit. Comparing with main
# instead of a committed baseline means pull requests that lower a count don't conflict:
#
#   tools/ci/check-quality-ratchets.sh              # compare with origin/main
#   tools/ci/check-quality-ratchets.sh <ref>        # or with another commit
#
# Rust counts look only at non-test code: each file up to its first #[cfg(test)].
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
max_lines=800

non_test_rust() {
  git ls-files '*.rs' | while read -r f; do awk '/#\[cfg\(test\)\]/{exit} {print FILENAME": "$0}' "$f"; done
}
count() { grep -cE "$1" <<<"$rust" || true; }
files_with() { git ls-files '*.rs' | grep -vE "$2" | xargs grep -lE "$1" 2>/dev/null | wc -l | tr -d ' ' || true; }

# One function per count, so this runs on macOS's bash 3.2, which has no associative arrays.
metric() {
  case "$1" in
    # #221: reading another service's JSON by field name.
    json_field_index) count '\["[a-z_]+"\]' ;;
    # #223: hyper clients and in-cluster Kubernetes clients built outside the shared libraries.
    hyper_clients) files_with 'Client::builder\(TokioExecutor::new\(\)\)' '^libs/http/' ;;
    kube_clients) files_with 'KUBERNETES_SERVICE_HOST' '^libs/kube/' ;;
    # #224: errors flattened to strings.
    string_results) count 'Result<[^>]*, String>' ;;
    stringified_errors) count 'map_err\(\|[a-z_]+\| [a-z_]+\.to_string\(\)\)' ;;
    # #227: files over the size limit.
    large_files) git ls-files '*.rs' '*.ts' '*.tsx' | grep -v __generated__ | xargs wc -l |
      awk -v m=$max_lines '$2 != "total" && $1 > m' | wc -l | tr -d ' ' ;;
    # ... and how far over it they are together, so a big file can only shrink.
    large_file_excess_lines) git ls-files '*.rs' '*.ts' '*.tsx' | grep -v __generated__ | xargs wc -l |
      awk -v m=$max_lines '$2 != "total" && $1 > m { s += $1 - m } END { print s + 0 }' ;;
  esac
}
metrics="hyper_clients json_field_index kube_clients large_file_excess_lines large_files string_results stringified_errors"

# Every count, one "name value" line each, for the tree in the current directory.
counts() {
  rust=$(non_test_rust)
  for k in $metrics; do echo "$k $(metric "$k")"; done
}

base=$(git merge-base HEAD "${1:-origin/main}")
tree=$(mktemp -d)
trap 'git worktree remove --force "$tree" >/dev/null 2>&1 || true' EXIT
# Without hooks: a local checkout hook may start Bazel.
git -c core.hooksPath=/dev/null worktree add --detach --quiet "$tree" "$base"
before=$( (cd "$tree" && counts) )
now=$(counts)

status=0
while read -r key was; do
  n=$(awk -v k="$key" '$1 == k { print $2 }' <<<"$now")
  if [ "$n" -gt "$was" ]; then
    echo "::error::$key is $n, up from $was on main: see the code-quality issues (#221 to #227) for what to do instead" >&2
    status=1
  else
    echo "$key $n (main: $was)"
  fi
done <<<"$before"
exit $status

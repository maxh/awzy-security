#!/usr/bin/env bash
# Counts patterns the code-quality pass is removing (#221 to #227) and fails if any grows past its
# baseline in tools/ci/quality-baseline.txt, or if a file grows past the size limit. When a count
# drops, the baseline must drop with it, so the gain sticks:
#
#   tools/ci/check-quality-ratchets.sh            # check
#   tools/ci/check-quality-ratchets.sh --update   # write the current counts as the baseline
#
# Rust counts look only at non-test code: each file up to its first #[cfg(test)].
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
baseline=tools/ci/quality-baseline.txt
max_lines=800

non_test_rust() {
  git ls-files '*.rs' | while read -r f; do awk '/#\[cfg\(test\)\]/{exit} {print FILENAME": "$0}' "$f"; done
}
rust=$(non_test_rust)
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
    *) echo "" ;;
  esac
}
metrics="hyper_clients json_field_index kube_clients large_file_excess_lines large_files string_results stringified_errors"

if [ "${1:-}" = "--update" ]; then
  { echo "# Written by tools/ci/check-quality-ratchets.sh --update. Counts may only go down."
    for k in $metrics; do echo "$k $(metric "$k")"; done
  } >"$baseline"
  cat "$baseline"
  exit 0
fi

status=0
while read -r key limit; do
  case "$key" in ''|'#'*) continue ;; esac
  n=$(metric "$key")
  if [ -z "$n" ]; then echo "::error::$baseline names $key, which this script doesn't count" >&2; status=1; continue; fi
  if [ "$n" -gt "$limit" ]; then
    echo "::error::$key is $n, up from $limit: see the code-quality issues (#221 to #227) for what to do instead" >&2
    status=1
  elif [ "$n" -lt "$limit" ]; then
    echo "::error::$key is down to $n from $limit. Lower it in $baseline (tools/ci/check-quality-ratchets.sh --update) so it stays down" >&2
    status=1
  else
    echo "$key $n"
  fi
done <"$baseline"
exit $status

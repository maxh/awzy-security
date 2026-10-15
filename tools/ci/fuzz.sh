#!/usr/bin/env bash
# Runs every cargo-fuzz target in the repository (#429) for AWZY_FUZZ_SECONDS each (60 by
# default). A crash fails the run, and its input is left in the target's fuzz/artifacts/ for CI
# to keep. Each target's corpus is in its fuzz/corpus/, which the nightly run caches.
# Needs a nightly toolchain and cargo-fuzz (`cargo install cargo-fuzz`).
#
#   tools/ci/fuzz.sh [path/to/fuzz ...]
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

seconds="${AWZY_FUZZ_SECONDS:-60}"
if [ "$#" -gt 0 ]; then
  dirs="$*"
else
  dirs="$(git ls-files '*/fuzz/Cargo.toml' | xargs -n1 dirname)"
fi

failed=0
for dir in $dirs; do
  for target in $(cd "$dir" && cargo +nightly fuzz list); do
    echo "::group::fuzz $dir $target (${seconds}s)"
    if (cd "$dir" && cargo +nightly fuzz run "$target" -- -max_total_time="$seconds"); then
      echo "::endgroup::"
      echo "ok: $dir $target"
    else
      echo "::endgroup::"
      failed=1
      echo "::error file=$dir/fuzz_targets/$target.rs::$target found a failing input; it's in $dir/artifacts/$target/"
    fi
  done
done
exit "$failed"

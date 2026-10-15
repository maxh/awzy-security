#!/usr/bin/env bash
# Checks every P model in the repository (ADR 0126): compiles each `.pproj` and runs each of its
# test cases for AWZY_P_SCHEDULES schedules (3000 by default). A broken property fails with the
# trace that broke it. Needs .NET 8 and the P toolchain (`dotnet tool install --global P`).
#
#   tools/ci/check-p-models.sh [path/to/Model.pproj ...]
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

schedules="${AWZY_P_SCHEDULES:-3000}"
if [ "$#" -gt 0 ]; then
  projects="$*"
else
  projects="$(git ls-files --cached --others --exclude-standard '*.pproj')"
fi
[ -n "$projects" ] || { echo "no P models"; exit 0; }

failed=0
for pproj in $projects; do
  dir="$(dirname "$pproj")"
  echo "::group::compile $pproj"
  (cd "$dir" && p compile --pproj "$(basename "$pproj")")
  echo "::endgroup::"
  # The test cases are declared in the files the project lists; their names are unique to it.
  tests="$(
    sed -n 's|.*<PFile>\(.*\)</PFile>.*|\1|p' "$pproj" \
      | while read -r f; do cat "$dir/$f"; done \
      | sed -n 's/^test \([A-Za-z0-9_]*\).*/\1/p'
  )"
  for tc in $tests; do
    out="$(mktemp -d)"
    if (cd "$dir" && p check -tc "$tc" -i "$schedules" -o "$out" > "$out/log.txt" 2>&1); then
      echo "ok: $pproj $tc ($(grep -o 'Explored [0-9]* schedules' "$out/log.txt" | head -1))"
    else
      failed=1
      echo "::error file=$pproj::$tc failed"
      cat "$out/log.txt"
    fi
    # PChecker exits 0 when it finds a bug, so look for one too.
    if trace="$(find "$out" -name '*_0_0.txt' | head -1)" && [ -n "$trace" ]; then
      failed=1
      echo "::error file=$pproj::$tc broke a property: $(grep -h -o 'Assertion Failed.*' "$trace" | head -1)"
      grep -vE '<(DequeueLog|MonitorLog|StateLog|ActionLog|ReceiveLog)>' "$trace" | tail -200
    fi
    rm -rf "$out"
  done
done
exit "$failed"

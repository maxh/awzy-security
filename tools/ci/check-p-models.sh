#!/usr/bin/env bash
# Checks every P model in the repository (ADR 0126): compiles each `.pproj` and runs each of its
# test cases for AWZY_P_SCHEDULES schedules (3000 by default). A broken property fails with the
# trace that broke it. Needs .NET 8 and the P toolchain (`dotnet tool install --global P`).
#
# Then it puts back each bug the model's `bugs.p` lists, one at a time, and fails unless the test
# named for it breaks: a model that stops catching a bug it found has gone soft. AWZY_P_BUGS=0
# skips that. A bug the checker needs more schedules to reach says so (`within <n>`), and is
# checked over that many only when AWZY_P_SLOW_BUGS=1, as the nightly run sets.
#
#   tools/ci/check-p-models.sh [path/to/Model.pproj ...]
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

schedules="${AWZY_P_SCHEDULES:-3000}"
check_bugs="${AWZY_P_BUGS:-1}"
slow_bugs="${AWZY_P_SLOW_BUGS:-0}"
if [ "$#" -gt 0 ]; then
  projects="$*"
else
  projects="$(git ls-files --cached --others --exclude-standard '*.pproj')"
fi
[ -n "$projects" ] || { echo "no P models"; exit 0; }

# A bug put back is undone however the script ends.
bugs_file=""
restore() { if [ -n "$bugs_file" ]; then mv "$bugs_file.orig" "$bugs_file"; bugs_file=""; fi; }
trap restore EXIT

compile() {
  local dir="$1" pproj="$2"
  echo "::group::compile $pproj"
  (cd "$dir" && p compile --pproj "$(basename "$pproj")" < /dev/null)
  echo "::endgroup::"
}

# Runs test case $2 in $1's project, already compiled. Sets `broke` to the assertion that failed,
# empty if none did, and `trace` to the trace's last lines; returns non-zero if the checker
# failed without finding a bug.
run_check() {
  local dir="$1" tc="$2" schedules="${3:-$schedules}" out file status=0
  out="$(mktemp -d)"
  broke=""
  trace=""
  (cd "$dir" && p check -tc "$tc" -i "$schedules" -o "$out" < /dev/null > "$out/log.txt" 2>&1) || status=$?
  explored="$(grep -o 'Explored [0-9]* schedules' "$out/log.txt" | head -1 || true)"
  # PChecker exits 0 when it finds a bug, so look for one.
  if file="$(find "$out" -name '*_0_0.txt' | head -1)" && [ -n "$file" ]; then
    broke="$(grep -h -o 'Assertion Failed.*' "$file" | head -1 || true)"
    [ -n "$broke" ] || broke="$(grep -h -o '<ErrorLog>.*' "$file" | head -1 || echo "a failure")"
    trace="$(grep -vE '<(DequeueLog|MonitorLog|StateLog|ActionLog|ReceiveLog)>' "$file" | tail -200)"
  elif [ "$status" -ne 0 ]; then
    trace="$(cat "$out/log.txt")"
  fi
  rm -rf "$out"
  [ -n "$broke" ] || return "$status"
}

failed=0
for pproj in $projects; do
  dir="$(dirname "$pproj")"
  compile "$dir" "$pproj"
  # The test cases are declared in the files the project lists; their names are unique to it.
  tests="$(
    sed -n 's|.*<PFile>\(.*\)</PFile>.*|\1|p' "$pproj" \
      | while read -r f; do cat "$dir/$f"; done \
      | sed -n 's/^test \([A-Za-z0-9_]*\).*/\1/p'
  )"
  for tc in $tests; do
    if ! run_check "$dir" "$tc"; then
      failed=1
      echo "::error file=$pproj::$tc failed"
      echo "$trace"
    elif [ -n "$broke" ]; then
      failed=1
      echo "::error file=$pproj::$tc broke a property: $broke"
      echo "$trace"
    else
      echo "ok: $pproj $tc ($explored)"
    fi
  done

  # Each bug back in turn: `// bug <n> breaks <test>[ within <schedules>]: ...` in bugs.p, read
  # by Bug().
  [ "$check_bugs" != 0 ] && [ -f "$dir/bugs.p" ] || continue
  while read -r n tc within; do
    within="${within:-$schedules}"
    if [ "$within" -gt "$schedules" ]; then
      if [ "$slow_bugs" = 0 ]; then
        echo "skipped: $pproj bug $n needs $within schedules; the nightly run checks it"
        continue
      fi
    else
      within="$schedules"
    fi
    bugs_file="$dir/bugs.p"
    cp "$bugs_file" "$bugs_file.orig"
    sed -i.tmp 's/^fun Bug(): int { return 0; }$/fun Bug(): int { return '"$n"'; }/' "$bugs_file"
    rm -f "$bugs_file.tmp"
    grep -q "return $n; }" "$bugs_file" || { echo "::error file=$bugs_file::no 'fun Bug(): int { return 0; }' to switch"; exit 1; }
    compile "$dir" "$pproj"
    if ! run_check "$dir" "$tc" "$within"; then
      failed=1
      echo "::error file=$bugs_file::with bug $n back, $tc failed to run"
      echo "$trace"
    elif [ -z "$broke" ]; then
      failed=1
      echo "::error file=$bugs_file::with bug $n back, $tc still passed ($explored): the model no longer catches it"
    else
      echo "ok: $pproj bug $n breaks $tc: $broke"
    fi
    restore
  done < <(sed -n 's|^// bug \([0-9]*\) breaks \([A-Za-z0-9_]*\)\( within \([0-9]*\)\)\{0,1\}:.*|\1 \2 \4|p' "$dir/bugs.p")
  # Leave the project compiled as it is.
  compile "$dir" "$pproj"
done
exit "$failed"

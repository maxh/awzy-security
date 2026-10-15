#!/usr/bin/env bash
# Warns about Rust that a P model covers (ADR 0126) changing while its model doesn't: a `.rs`
# file changed since $1 (a commit) with a `.p` of the same name beside it that didn't. Only
# warns, since plenty of changes don't touch the protocol; the pull request should say why.
#
#   tools/ci/check-p-drift.sh <base>
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

base="${1:?usage: check-p-drift.sh <base commit>}"
changed="$(git diff --name-only "$base"...HEAD)"
drifted=0
for rs in $(grep '\.rs$' <<< "$changed" || true); do
  model="${rs%.rs}.p"
  [ -f "$model" ] || continue
  grep -qxF "$model" <<< "$changed" && continue
  drifted=1
  echo "::warning file=$rs::$rs changed but its model, $model, didn't. If the protocol changed, change the model too (the model-protocol skill); if not, say so in the pull request."
done
[ "$drifted" = 0 ] && echo "every changed file with a P model beside it changed its model too"
exit 0

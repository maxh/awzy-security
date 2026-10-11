#!/usr/bin/env bash
# Bootstrap's level and the chart's bootstrap.required are equal (ADR 0098): a change that makes
# the chart need something new from bootstrap raises both, so a release never installs onto a
# bootstrap that lacks it, and never refuses one that has it.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
level=$(sed -nE 's/^ *bootstrap_level *= *([0-9]+).*/\1/p' deploy/terraform/bootstrap/main.tf)
required=$(awk '/^bootstrap:/{f=1; next} f && /^[^ ]/{f=0} f && $1=="required:"{print $2}' deploy/helm/awzy/values.yaml)
[ -n "$level" ] && [ -n "$required" ] || { echo "can't find bootstrap_level or bootstrap.required" >&2; exit 1; }
if [ "$level" != "$required" ]; then
  echo "bootstrap_level is $level (deploy/terraform/bootstrap/main.tf) but the chart requires $required (deploy/helm/awzy/values.yaml); raise them together" >&2
  exit 1
fi
echo "bootstrap level $level, required $required"

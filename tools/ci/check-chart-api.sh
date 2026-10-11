#!/usr/bin/env bash
# The chart as a Kubernetes API server reads it: a server-side dry run of every object it renders,
# against a throwaway kind cluster. It catches what helm lint can't, above all an admission
# policy's CEL that doesn't compile, which made every sync after #208 fail on every install.
# Kinds from CRDs a bare cluster doesn't have (the ClickHouse operator's, Gateway API, External
# Secrets) are skipped.
#
#   tools/ci/check-chart-api.sh             # makes a kind cluster, and deletes it after
#   KUBE_CONTEXT=name tools/ci/check-chart-api.sh   # uses that cluster: one without Kyverno, which
#                                                   # refuses the example values' placeholder registry
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

if [ -z "${KUBE_CONTEXT:-}" ]; then
  cluster=awzy-chart-check
  kind create cluster --name "$cluster" --wait 120s >/dev/null
  trap 'kind delete cluster --name "$cluster" >/dev/null 2>&1' EXIT
  KUBE_CONTEXT="kind-$cluster"
fi
k() { kubectl --context "$KUBE_CONTEXT" "$@"; }

rendered=$(mktemp)
helm template awzy deploy/helm/awzy -f deploy/helm/awzy/values-example.yaml > "$rendered"

# The chart's namespaces are bootstrap's to make; a dry run needs them to exist.
for ns in $(grep -E '^  namespace: ' "$rendered" | awk '{print $2}' | sort -u); do
  k get namespace "$ns" >/dev/null 2>&1 || k create namespace "$ns" >/dev/null
done

errors=$(k apply --dry-run=server -f "$rendered" 2>&1 >/dev/null |
  grep -v -E 'no matches for kind|ensure CRDs are installed first' || true)
if [ -n "$errors" ]; then
  echo "the API server refuses part of the chart:" >&2
  echo "$errors" >&2
  exit 1
fi
echo "the API server accepts the chart ($(grep -c '^kind: ' "$rendered") objects rendered)"

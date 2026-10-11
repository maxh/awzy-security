#!/usr/bin/env bash
# Checks that a Kubernetes cluster can take bootstrap, for clients who run it on a cluster they
# already have (platform plan, "Getting into a client account"; ADR 0015). Read-only: it only
# reads through kubectl, and changes nothing.
#
#   deploy/terraform/bootstrap/preflight.sh [--context NAME] [--contract FILE]
#
# --contract is the cloud layer's output (`tofu output -json install_contract > contract.json`);
# with it, the check also covers the storage provisioner and the size profile. Exits 1 if any
# check fails; warnings don't fail it.
set -uo pipefail

context=() contract=""
while [ $# -gt 0 ]; do
  case "$1" in
    --context) context=(--context "$2"); shift 2 ;;
    --contract) contract="$2"; shift 2 ;;
    *) echo "usage: $0 [--context NAME] [--contract FILE]" >&2; exit 2 ;;
  esac
done

k() { kubectl "${context[@]}" "$@"; }
failed=0
ok() { echo "ok    $*"; }
warn() { echo "warn  $*"; }
fail() { echo "FAIL  $*"; failed=1; }

# Kubernetes versions in EKS, GKE, and AKS standard support.
min_minor=33

if ! version=$(k version -o json 2>/dev/null | jq -r '.serverVersion | "\(.major).\(.minor)"'); then
  fail "can't reach the cluster with kubectl${context[1]:+ (context ${context[1]})}"
  exit 1
fi
minor=${version#*.}; minor=${minor%%[!0-9]*}
if [ "${version%%.*}" = 1 ] && [ "$minor" -ge "$min_minor" ]; then
  ok "Kubernetes $version"
else
  fail "Kubernetes $version; bootstrap needs 1.$min_minor or later"
fi

# Bootstrap installs cluster-wide components (CRDs, webhooks, ClusterRoles).
if [ "$(k auth can-i '*' '*' --all-namespaces 2>/dev/null)" = yes ]; then
  ok "cluster-admin"
else
  fail "not cluster-admin: bootstrap creates CRDs, ClusterRoles, and namespaces"
fi

# At least two nodes, in two zones, so replicas and their disruption budgets have somewhere to go.
nodes=$(k get nodes -o json)
ready=$(jq '[.items[] | select(any(.status.conditions[]; .type == "Ready" and .status == "True"))] | length' <<<"$nodes")
zones=$(jq '[.items[].metadata.labels["topology.kubernetes.io/zone"] // empty] | unique | length' <<<"$nodes")
[ "$ready" -ge 2 ] && ok "$ready ready nodes" || fail "$ready ready nodes; awzy needs at least 2"
[ "$zones" -ge 2 ] && ok "nodes in $zones zones" || warn "nodes in $zones zone(s); a zone outage takes the install down"

# Allocatable capacity across Linux nodes, against what the size profile requests.
cpu_m=$(jq '[.items[] | select(.metadata.labels["kubernetes.io/os"] == "linux") | .status.allocatable.cpu
  | if endswith("m") then (.[:-1] | tonumber) else (tonumber * 1000) end] | add // 0' <<<"$nodes")
mem_mi=$(jq '[.items[] | select(.metadata.labels["kubernetes.io/os"] == "linux") | .status.allocatable.memory
  | if endswith("Ki") then (.[:-2] | tonumber / 1024) elif endswith("Mi") then (.[:-2] | tonumber)
    elif endswith("Gi") then (.[:-2] | tonumber * 1024) else (tonumber / 1048576) end] | add // 0 | floor' <<<"$nodes")
profile=small
[ -n "$contract" ] && profile=$(jq -r '.size_profile // "small"' "$contract")
case "$profile" in
  # Measured on the example install: two t3.large nodes run small with room for its autoscalers.
  small) need_cpu=3500 need_mem=12288 ;;
  *) need_cpu=0 need_mem=0; warn "size profile $profile isn't sized yet; skipping the capacity check" ;;
esac
if [ "$need_cpu" -gt 0 ]; then
  if [ "$cpu_m" -ge "$need_cpu" ] && [ "$mem_mi" -ge "$need_mem" ]; then
    ok "allocatable ${cpu_m}m CPU, ${mem_mi}Mi memory (profile $profile)"
  else
    fail "allocatable ${cpu_m}m CPU, ${mem_mi}Mi memory; profile $profile needs ${need_cpu}m and ${need_mem}Mi"
  fi
fi

# The storage class bootstrap creates needs its provisioner's CSI driver.
if [ -n "$contract" ]; then
  provisioner=$(jq -r '.storage_class.provisioner' "$contract")
  case "$provisioner" in
    kubernetes.io/*) ok "storage provisioner $provisioner (in-tree)" ;;
    *) k get csidriver "$provisioner" >/dev/null 2>&1 && ok "CSI driver $provisioner" ||
      fail "no CSI driver $provisioner for ClickHouse's storage class; install it first" ;;
  esac
fi

# What bootstrap installs: if one is already on the cluster and bootstrap didn't put it there, a
# second copy would fight the first over its CRDs and webhooks.
release_here() { k -n "$1" get secret -l "owner=helm,name=$2,status=deployed" -o name 2>/dev/null | grep -q .; }
while read -r crd component namespace release; do
  component=${component//-/ }
  if ! k get crd "$crd" >/dev/null 2>&1; then
    ok "$component not installed yet"
  elif release_here "$namespace" "$release"; then
    ok "$component installed by bootstrap"
  else
    fail "$component is already installed (CRD $crd), not by bootstrap; remove it or ask awzy support"
  fi
done <<'EOF'
applications.argoproj.io Argo-CD awzy-ops argo-cd
clusterpolicies.kyverno.io Kyverno kyverno kyverno
externalsecrets.external-secrets.io External-Secrets external-secrets external-secrets
clickhouseinstallations.clickhouse.altinity.com ClickHouse-operator awzy-ops clickhouse-operator
envoyproxies.gateway.envoyproxy.io Envoy-Gateway envoy-gateway-system envoy-gateway
certificates.cert-manager.io cert-manager cert-manager cert-manager
EOF

# Gateway API CRDs come with Envoy Gateway; another copy may be an older version.
if k get crd gateways.gateway.networking.k8s.io >/dev/null 2>&1 && ! release_here envoy-gateway-system envoy-gateway; then
  warn "Gateway API CRDs are already installed; Envoy Gateway's chart brings its own version"
fi

# metrics-server: bootstrap installs it unless the cluster already has the metrics API.
if ! k get apiservice v1beta1.metrics.k8s.io >/dev/null 2>&1; then
  ok "no metrics API yet; bootstrap installs metrics-server"
elif release_here kube-system metrics-server; then
  ok "metrics-server installed by bootstrap"
else
  warn "the cluster already serves the metrics API; set metrics_server = false in bootstrap"
fi

for ns in awzy-system awzy-data awzy-portal awzy-ops; do
  if k get namespace "$ns" >/dev/null 2>&1 && ! release_here awzy-ops awzy-config; then
    warn "namespace $ns exists, but bootstrap hasn't run here; awzy expects to own it"
  fi
done

[ "$failed" = 0 ] && echo "preflight passed" || echo "preflight failed"
exit "$failed"

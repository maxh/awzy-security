#!/usr/bin/env bash
# A game day (#353): takes one dependency away from an install under load, and records what the
# PDPs decide, whether a new PDP becomes ready, what records are dropped, and how long recovery
# takes. Results go in docs/perf/.
#
#   tools/gameday/gameday.sh <kube-context> <scenario> [away-seconds] [out-dir]
#
# Scenarios:
#   context, challenge, policy  the service scaled to zero
#   kafka                       in-cluster Kafka: the broker pods deleted, with the Strimzi operator stopped
#   kafka-broker                Amazon MSK: one broker rebooted (AWZY_TEST_MSK_CLUSTER_ARN)
#   postgres                    Google Cloud: the Cloud SQL instance restarted (AWZY_TEST_GCP_PROJECT,
#                               AWZY_TEST_CLOUD_SQL_INSTANCE). AWS: the Aurora writer rebooted
#                               (AWZY_TEST_AURORA_INSTANCE), or failed over to a replica
#                               (AWZY_TEST_AURORA_CLUSTER) where there is one
#   node                        the node running the most PDPs drained
#   cold                        awzy-context away, then every PDP restarted: the cold start
#
# Load: open loop at AWZY_TEST_GAMEDAY_RATE checks/s (default 200) over 8 connections, a line a
# second. Timeline: 60 s of load, the dependency away, one PDP pod deleted 30 s into it so its replacement
# starts without the dependency, the dependency back, then 120 s more. Argo CD's automated sync
# is off for the run, so neither self-heal nor a new release undoes the outage, and is put back
# as it was at the end, even on failure.
set -euo pipefail

ctx=${1:?kube context}
scenario=${2:?scenario}
away=${3:-180}
out=${4:-gameday-$scenario-$(date -u +%Y%m%dT%H%M%SZ)}
sys=awzy-system
k() { kubectl --context "$ctx" "$@"; }
now() { date -u +%H:%M:%S; }
note() { echo "$(now)  $*" | tee -a "$out/events.log"; }
mkdir -p "$out"

argo_pause() {
  k -n awzy-ops get application awzy -o jsonpath='{.spec.syncPolicy}' >"$out/sync-policy.json"
  k -n awzy-ops patch application awzy --type json -p '[{"op":"remove","path":"/spec/syncPolicy/automated"}]' >/dev/null
}

argo_resume() {
  [[ -s $out/sync-policy.json ]] &&
    k -n awzy-ops patch application awzy --type merge -p "{\"spec\":{\"syncPolicy\":$(cat "$out/sync-policy.json")}}" >/dev/null
}

# A pod's metrics through a port-forward, which goes through the kubelet: EKS's API server can't
# reach pod IPs, so its pod proxy hangs there.
metrics() {
  local log port="" pf
  log=$(mktemp)
  k -n "$sys" port-forward "pod/$1" :8080 >"$log" 2>&1 &
  pf=$!
  for _ in $(seq 20); do
    port=$(sed -n 's/^Forwarding from 127.0.0.1:\([0-9]*\) .*/\1/p' "$log" | head -1)
    [[ -n $port ]] && break
    sleep 0.1
  done
  [[ -n $port ]] && curl -s -m 2 "http://127.0.0.1:$port/metrics"
  kill "$pf" 2>/dev/null
  rm -f "$log"
}

# What each awzy workload has ready, and each PDP's freshness and drop counters, every 5 s.
sample() {
  while :; do
    {
      echo "== $(now)"
      k -n "$sys" get deploy --no-headers -o custom-columns='N:.metadata.name,R:.status.readyReplicas,W:.spec.replicas' 2>&1
      for pod in $(k -n "$sys" get pod -l app.kubernetes.io/name=awzy-pdp -o name 2>/dev/null); do
        echo "-- ${pod#pod/}"
        metrics "${pod#pod/}" 2>&1 |
          grep -E '^awzy_(ready|pdp_(.*_age_seconds|.*dropped_total|check_errors_total|containments_stale_served_total|resource_lookups_total))' || true
      done
    } >>"$out/samples.log"
    sleep 5
  done
}

# pdp_loadtest from the PDP's own image, with the service token its callers send (ADR 0055).
start_load() {
  local image seconds=$1
  image=$(k -n "$sys" get deploy awzy-pdp -o jsonpath='{.spec.template.spec.containers[0].image}')
  k -n "$sys" delete pod gameday-load --ignore-not-found --wait >/dev/null
  k apply -f - >/dev/null <<EOF
apiVersion: v1
kind: Pod
metadata: { name: gameday-load, namespace: $sys, labels: { app.kubernetes.io/name: gameday-load } }
spec:
  serviceAccountName: awzy
  restartPolicy: Never
  imagePullSecrets: [{ name: awzy-registry }]
  securityContext: { runAsNonRoot: true, runAsUser: 65532, runAsGroup: 65532, seccompProfile: { type: RuntimeDefault } }
  containers:
    - name: load
      image: $image
      command: [/usr/local/bin/pdp_loadtest, "http://awzy-pdp.$sys.svc:8080", "8", "$seconds", "${AWZY_TEST_GAMEDAY_RATE:-200}", "1"]
      securityContext: { allowPrivilegeEscalation: false, capabilities: { drop: [ALL] }, readOnlyRootFilesystem: true }
      resources: { requests: { cpu: 250m, memory: 64Mi } }
      volumeMounts: [{ name: tokens, mountPath: /var/run/secrets/awzy/tokens, readOnly: true }]
  volumes:
    - name: tokens
      projected: { sources: [{ serviceAccountToken: { audience: awzy-pdp, expirationSeconds: 600, path: awzy-pdp } }] }
EOF
  k -n "$sys" wait pod/gameday-load --for=condition=Ready --timeout=120s >/dev/null
}

take_away() {
  case $scenario in
    context | challenge | policy) k -n "$sys" scale deploy "awzy-$scenario" --replicas=0 >/dev/null ;;
    cold) k -n "$sys" scale deploy awzy-context --replicas=0 >/dev/null ;;
    kafka)
      k -n kafka scale deploy strimzi-cluster-operator --replicas=0 >/dev/null
      k -n kafka wait --for=delete pod -l name=strimzi-cluster-operator --timeout=120s >/dev/null || true
      k -n kafka delete pod -l strimzi.io/kind=Kafka,strimzi.io/broker-role=true --wait=false >/dev/null
      ;;
    kafka-broker)
      aws kafka reboot-broker --cluster-arn "${AWZY_TEST_MSK_CLUSTER_ARN:?}" --broker-ids 1 >/dev/null
      ;;
    postgres)
      if [[ -n ${AWZY_TEST_AURORA_CLUSTER:-} ]]; then
        aws rds failover-db-cluster --db-cluster-identifier "$AWZY_TEST_AURORA_CLUSTER" >/dev/null
      elif [[ -n ${AWZY_TEST_AURORA_INSTANCE:-} ]]; then
        aws rds reboot-db-instance --db-instance-identifier "$AWZY_TEST_AURORA_INSTANCE" >/dev/null
      else
        gcloud sql instances restart "${AWZY_TEST_CLOUD_SQL_INSTANCE:?}" --project "${AWZY_TEST_GCP_PROJECT:?}" --async >/dev/null
      fi
      ;;
    node)
      node=$(k -n "$sys" get pod -l app.kubernetes.io/name=awzy-pdp -o jsonpath='{range .items[*]}{.spec.nodeName}{"\n"}{end}' | sort | uniq -c | sort -rn | awk 'NR==1{print $2}')
      echo "$node" >"$out/node"
      # The load pod has no controller, so drain would refuse the node; it stays where it is.
      k drain "$node" --ignore-daemonsets --delete-emptydir-data --timeout="${away}s" \
        --pod-selector='app.kubernetes.io/name!=gameday-load' >>"$out/events.log" 2>&1 &
      ;;
    *) echo "unknown scenario: $scenario" >&2 && exit 2 ;;
  esac
}

bring_back() {
  case $scenario in
    context | challenge | policy) k -n "$sys" scale deploy "awzy-$scenario" --replicas="$replicas" >/dev/null ;;
    cold) k -n "$sys" scale deploy awzy-context --replicas="$replicas" >/dev/null ;;
    kafka) k -n kafka scale deploy strimzi-cluster-operator --replicas=1 >/dev/null ;;
    kafka-broker | postgres) ;;
    node) k uncordon "$(cat "$out/node")" >/dev/null ;;
  esac
}

restore() {
  bring_back || true
  argo_resume || true
  k -n "$sys" delete pod gameday-load --ignore-not-found --wait=false >/dev/null || true
  [[ -n ${sampler:-} ]] && kill "$sampler" 2>/dev/null || true
}
trap restore EXIT

case $scenario in
  context | challenge | policy) replicas=$(k -n "$sys" get deploy "awzy-$scenario" -o jsonpath='{.spec.replicas}') ;;
  cold) replicas=$(k -n "$sys" get deploy awzy-context -o jsonpath='{.spec.replicas}') ;;
esac

# A rollout still under way would land in the measurements.
for d in $(k -n "$sys" get deploy -o name); do
  k -n "$sys" rollout status "$d" --timeout=600s >/dev/null
done
argo_pause
started=$(date -u '+%Y-%m-%d %H:%M:%S')
note "automated sync off; load starting ($scenario, ${away}s away)"
start_load $((60 + away + 120 + 30))
sample &
sampler=$!
sleep 60

note "taking $scenario away"
take_away
sleep 30
if [[ $scenario == cold ]]; then
  note "restarting every PDP"
  k -n "$sys" delete pod -l app.kubernetes.io/name=awzy-pdp --wait=false >/dev/null
else
  victim=$(k -n "$sys" get pod -l app.kubernetes.io/name=awzy-pdp -o name | head -1)
  note "deleting ${victim#pod/}, so a new PDP starts without $scenario"
  k -n "$sys" delete "$victim" --wait=false >/dev/null
fi
sleep $((away - 30))

note "bringing $scenario back"
bring_back
sleep 120
note "done; waiting for the load to finish"
# Read once the pod is done: a log followed for the whole run can lose its connection.
k -n "$sys" wait pod/gameday-load --for=jsonpath='{.status.phase}'=Succeeded --timeout=120s >/dev/null || true
k -n "$sys" logs gameday-load >"$out/timeline.log" 2>&1
k -n "$sys" get pod -l app.kubernetes.io/name=awzy-pdp -o wide >>"$out/events.log"

# Every check the load sent has a decision record, so ClickHouse's count by 10 s against the
# timeline's shows what was lost. ClickHouse reads the topic each second; give it a minute.
sleep 60
k -n awzy-data exec chi-awzy-awzy-0-0-0 -- clickhouse-client -d awzy -q "
  SELECT toStartOfInterval(at, INTERVAL 10 SECOND) AS t, count()
  FROM decisions WHERE at >= '$started' AND context LIKE '%loadtest%'
  GROUP BY t ORDER BY t" >"$out/records.log"
note "records: $(awk '{n += $3} END {print n}' "$out/records.log")"

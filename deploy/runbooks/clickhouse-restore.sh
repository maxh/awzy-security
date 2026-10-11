#!/usr/bin/env bash
# Restores ClickHouse's decisions and events from a backup (ADR 0105), then has Kafka refill
# everything after it, so nothing between the backup and now is lost while Kafka still holds it
# (7 days).
#
#   deploy/runbooks/clickhouse-restore.sh [--context NAME] [--backup WEEK/NAME] --drill
#   deploy/runbooks/clickhouse-restore.sh [--context NAME] [--backup WEEK/NAME] --yes
#
# --drill restores into a database of its own (awzy_drill) and refills it through a consumer group
# of its own, compares it with the live tables, and removes it: the live install isn't touched.
# --yes restores the live tables: for a ClickHouse that lost its data. It stops ClickHouse's Kafka
# consumers, replaces decisions and events with the backup, rewinds the consumer group to just
# before the backup, and starts the consumers again. Rows the live tables had that the backup
# doesn't, and that Kafka no longer holds, are gone.
#
# The newest backup unless --backup names one (`2026-W41/20261011T204107Z`). Needs kubectl access
# to the install; the Kafka rewind runs in a short-lived pod in --kafka-namespace (default:
# default), whose image is Apache Kafka's own.
set -euo pipefail

context=() backup="" mode="" kafka_ns=default
while [ $# -gt 0 ]; do
  case "$1" in
    --context) context=(--context "$2"); shift 2 ;;
    --backup) backup="$2"; shift 2 ;;
    --kafka-namespace) kafka_ns="$2"; shift 2 ;;
    --drill) mode=drill; shift ;;
    --yes) mode=live; shift ;;
    *) mode=""; break ;;
  esac
done
[ -n "$mode" ] || { echo "usage: $0 [--context NAME] [--backup WEEK/NAME] (--drill | --yes)" >&2; exit 2; }

ns=awzy-data db=awzy
# A rewind starts this long before the backup, so rows still in Kafka's queue when it ran come back.
margin_minutes=10
kafka_image="apache/kafka:4.1.1@sha256:0bc1bb2478f45b6cea78864df86acdc11e8df2c5172477819a4d12942cbe5d40"
topics=(awzy.decisions.v1 awzy.events.v1)

k() { kubectl "${context[@]}" "$@"; }
pod=$(k -n "$ns" get pods -l clickhouse.altinity.com/chi=awzy -o jsonpath='{.items[0].metadata.name}')
q() { k -n "$ns" exec "$pod" -- clickhouse-client --query "$1"; }
log() { echo "$(date -u +%H:%M:%S) $*"; }

url=$(k -n "$ns" get cronjob awzy-clickhouse-backup \
  -o jsonpath='{.spec.jobTemplate.spec.template.spec.containers[0].env[?(@.name=="BACKUP_URL")].value}')
[ -n "$url" ] || { echo "no awzy-clickhouse-backup CronJob: backups aren't on in this install" >&2; exit 1; }

if [ -z "$backup" ]; then
  newest=$(q "SELECT max(_path) FROM s3('$url/*/*/.backup', 'One') SETTINGS s3_throw_on_zero_files_match = 0")
  [ -n "$newest" ] || { echo "no backups under the install's backup URL" >&2; exit 1; }
  backup=$(echo "$newest" | awk -F/ '{print $(NF-2) "/" $(NF-1)}')
fi
stamp=$(basename "$backup" | cut -c1-16)
from=$(q "SELECT formatDateTime(parseDateTimeBestEffort('$stamp') - INTERVAL $margin_minutes MINUTE, '%Y-%m-%dT%H:%i:%S')")
log "backup $backup; Kafka refills from $from UTC"

# The live consumers' brokers and group, from the queue table the migrate Job made.
queue_sql=$(q "SELECT create_table_query FROM system.tables WHERE database = '$db' AND name = 'decisions_queue' FORMAT TSVRaw")
brokers=$(echo "$queue_sql" | sed -nE "s/.*kafka_broker_list = '([^']+)'.*/\1/p")
live_group=$(echo "$queue_sql" | sed -nE "s/.*kafka_group_name = '([^']+)'.*/\1/p")
tls=false
k -n "$ns" get chi awzy -o jsonpath='{.spec.configuration.files}' | grep -q security_protocol && tls=true

if [ "$mode" = drill ]; then
  target=awzy_drill
  group="awzy-drill-$(date -u +%Y%m%dT%H%M%SZ)"
  q "DROP DATABASE IF EXISTS $target SYNC"
  q "CREATE DATABASE $target"
  log "restoring into $target"
  q "RESTORE TABLE $db.decisions AS $target.decisions, TABLE $db.events AS $target.events FROM S3('$url/$backup')" >/dev/null
else
  target=$db
  group=$live_group
  log "stopping ClickHouse's Kafka consumers"
  q "DETACH TABLE $db.decisions_queue"
  q "DETACH TABLE $db.events_queue"
  q "DROP TABLE $db.decisions SYNC"
  q "DROP TABLE $db.events SYNC"
  log "restoring $db.decisions and $db.events"
  q "RESTORE TABLE $db.decisions, TABLE $db.events FROM S3('$url/$backup')" >/dev/null
fi

# What Kafka refills is removed from the restored tables first, so nothing appears twice.
for t in decisions events; do
  q "ALTER TABLE $target.$t DELETE WHERE at >= toDateTime64('$from', 3, 'UTC') SETTINGS mutations_sync = 1"
done

log "rewinding consumer group $group to $from"
tools="awzy-kafka-tools-$(date +%s)"
k -n "$kafka_ns" run "$tools" --image="$kafka_image" --restart=Never --command -- sleep 900 >/dev/null
trap 'k -n "$kafka_ns" delete pod "$tools" --wait=false >/dev/null 2>&1 || true' EXIT
k -n "$kafka_ns" wait --for=condition=Ready "pod/$tools" --timeout=180s >/dev/null
props=""
$tls && props="security.protocol=SSL"
k -n "$kafka_ns" exec "$tools" -- sh -c "echo '$props' > /tmp/client.properties"
topic_args=()
for t in "${topics[@]}"; do topic_args+=(--topic "$t"); done
k -n "$kafka_ns" exec "$tools" -- /opt/kafka/bin/kafka-consumer-groups.sh --bootstrap-server "$brokers" \
  --command-config /tmp/client.properties --group "$group" --reset-offsets --to-datetime "$from.000" \
  "${topic_args[@]}" --execute >/dev/null

if [ "$mode" = live ]; then
  q "ATTACH TABLE $db.decisions_queue"
  q "ATTACH TABLE $db.events_queue"
  log "consumers started; ClickHouse refills from Kafka. Done."
  exit 0
fi

# The drill's own consumers, made from the live ones' definitions with its database and group.
for t in decisions events; do
  queue=$(q "SELECT create_table_query FROM system.tables WHERE database = '$db' AND name = '${t}_queue' FORMAT TSVRaw")
  view=$(q "SELECT create_table_query FROM system.tables WHERE database = '$db' AND name = '${t}_mv' FORMAT TSVRaw")
  q "$(echo "$queue" | sed -e "s/$db\.${t}_queue/$target.${t}_queue/" -e "s/kafka_group_name = '$live_group'/kafka_group_name = '$group'/")"
  q "$(echo "$view" | sed -e "s/$db\.${t}_mv/$target.${t}_mv/" -e "s/TO $db\.$t /TO $target.$t /" -e "s/FROM $db\.${t}_queue/FROM $target.${t}_queue/")"
done

log "refilling $target from Kafka"
last=-1
for _ in $(seq 1 60); do
  sleep 10
  now=$(q "SELECT (SELECT count() FROM $target.decisions) + (SELECT count() FROM $target.events)")
  [ "$now" = "$last" ] && break
  last=$now
done

# Every live row older than a minute must be in the restore, once. The restore may also have rows
# the live tables no longer do: deleted after the backup, but still in Kafka, which refills them.
status=0
for t in decisions events; do
  read -r live missing extra dups < <(q "SELECT
      (SELECT count() FROM $db.$t WHERE at < now() - INTERVAL 1 MINUTE),
      (SELECT count() FROM $db.$t WHERE at < now() - INTERVAL 1 MINUTE AND id NOT IN (SELECT id FROM $target.$t)),
      (SELECT count() FROM $target.$t WHERE at < now() - INTERVAL 1 MINUTE AND id NOT IN (SELECT id FROM $db.$t)),
      (SELECT count() - uniqExact(id) FROM $target.$t)")
  echo "$t: $live live rows, $missing missing from the restore, $dups duplicated; $extra more in the restore (deleted from live, still in Kafka)"
  { [ "$missing" = 0 ] && [ "$dups" = 0 ]; } || status=1
done

for t in decisions events; do q "DROP TABLE $target.${t}_mv SYNC"; q "DROP TABLE $target.${t}_queue SYNC"; done
q "DROP DATABASE $target SYNC"
[ "$status" = 0 ] && log "drill passed: every live row was restored, none twice" || log "drill FAILED: rows are missing or duplicated"
exit "$status"

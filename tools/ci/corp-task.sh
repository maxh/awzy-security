#!/usr/bin/env bash
# Runs the corp portal's image once, as a one-off ECS task on the corp portal service's network
# (it reaches the database), with the given command, and fails unless it exits 0: `migrate` before
# a deploy (ADR 0087), and `channel set …` to move a release channel (ADR 0094).
#
#   tools/ci/corp-task.sh <task definition ARN, or "current" for the service's> <started-by> <command>...
set -euo pipefail
CLUSTER=awzy-corp-portal
definition=$1 started_by=$2
shift 2
if [ "$definition" = current ]; then
  definition=$(aws ecs describe-services --cluster "$CLUSTER" --services awzy-corp-portal \
    --query 'services[0].taskDefinition' --output text)
fi
network=$(aws ecs describe-services --cluster "$CLUSTER" --services awzy-corp-portal \
  --query 'services[0].networkConfiguration' --output json)
overrides=$(python3 -c 'import json, sys; print(json.dumps({"containerOverrides": [{"name": "corp-portal", "command": sys.argv[1:]}]}))' "$@")
task=$(aws ecs run-task --cluster "$CLUSTER" --task-definition "$definition" --launch-type FARGATE \
  --network-configuration "$network" --started-by "$started_by" --overrides "$overrides" \
  --query 'tasks[0].taskArn' --output text)
aws ecs wait tasks-stopped --cluster "$CLUSTER" --tasks "$task"
read -r code reason < <(aws ecs describe-tasks --cluster "$CLUSTER" --tasks "$task" --output text \
  --query "tasks[0].[containers[?name=='corp-portal'].exitCode | [0], stoppedReason]")
if [ "$code" != 0 ]; then
  echo "::error::The task ${task##*/} ($*) failed (exit $code: $reason). Its logs are in CloudWatch." >&2
  exit 1
fi

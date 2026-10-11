#!/usr/bin/env bash
# Deploys the corp portal release (ADR 0053) to the corp portal and the registry proxy on ECS
# (deploy/terraform/corp/corp-portal). Runs in CI after publish-images.sh, as awzy-ci-deploy.
#
# For each service: copy its task definition template (the revision Terraform registered, named
# in SSM by deploy-templates.tf) with the image set and register it, so Terraform's changes to a
# task definition reach the service with the next deploy. Then run the corp portal's new revision
# once as `awzy-corp-portal migrate` (ADR 0087): a failed migration stops the deploy here, before
# any service changes. Then point the fast channel at this commit's authz chart (ADR 0094), point
# each service at its new revision, and wait for it to settle. ECS's deployment circuit breaker
# rolls a failing deploy back; that shows up here as the old revision still running.
#
# CI runs for commits on main finish in any order, so a commit older than the one deployed is
# skipped rather than deployed over it. That needs the full history (fetch-depth: 0).
set -euo pipefail
: "${AWZY_CORP_PORTAL_RELEASE_REGISTRY:?}" "${AWZY_CORP_PORTAL_RELEASE_TAG:?}"
CLUSTER=awzy-corp-portal

deployed_image=$(aws ecs describe-services --cluster "$CLUSTER" --services awzy-corp-portal \
  --query 'services[0].taskDefinition' --output text |
  xargs -I{} aws ecs describe-task-definition --task-definition {} \
    --query "taskDefinition.containerDefinitions[?name=='corp-portal'].image | [0]" --output text)
deployed=${deployed_image##*:}
if [ "$deployed" != "$AWZY_CORP_PORTAL_RELEASE_TAG" ] && git cat-file -e "$deployed^{commit}" 2>/dev/null &&
  git merge-base --is-ancestor "$AWZY_CORP_PORTAL_RELEASE_TAG" "$deployed"; then
  echo "::notice::Skipped: ${deployed:0:12} is deployed and newer than ${AWZY_CORP_PORTAL_RELEASE_TAG:0:12}."
  exit 0
fi

# service:container:image repository
SERVICES=(
  "awzy-corp-portal:corp-portal:awzy/awzy-corp-portal"
  "awzy-registry:registry:awzy/awzy-registry"
)

declare -A wanted
for entry in "${SERVICES[@]}"; do
  IFS=: read -r service container repo <<< "$entry"
  current=$(aws ecs describe-services --cluster "$CLUSTER" --services "$service" --query 'services[0].taskDefinition' --output text)
  # The template; the running revision until Terraform has published one.
  template=$(aws ssm get-parameter --name "/awzy-corp-portal/task-definition-template/$service" \
    --query Parameter.Value --output text 2>/dev/null || echo "$current")
  aws ecs describe-task-definition --task-definition "$template" --query taskDefinition --output json > /tmp/td.json
  python3 - "$container" "$AWZY_CORP_PORTAL_RELEASE_REGISTRY/$repo:$AWZY_CORP_PORTAL_RELEASE_TAG" <<'PY'
import json, sys
container, image = sys.argv[1], sys.argv[2]
td = json.load(open("/tmp/td.json"))
found = False
for c in td["containerDefinitions"]:
    if c["name"] == container:
        c["image"] = image
        # What /version reports: the commit being deployed (the image's tag).
        env = [e for e in c.get("environment", []) if e["name"] != "AWZY_RELEASE_VERSION"]
        env.append({"name": "AWZY_RELEASE_VERSION", "value": image.rsplit(":", 1)[1]})
        c["environment"] = sorted(env, key=lambda e: e["name"])
        found = True
if not found:
    sys.exit(f"no container {container!r} in the task definition")
# Fields describe returns but register doesn't take.
for k in ["taskDefinitionArn", "revision", "status", "requiresAttributes", "compatibilities",
          "registeredAt", "registeredBy", "deregisteredAt"]:
    td.pop(k, None)
json.dump(td, open("/tmp/td.json", "w"))
PY
  new=$(aws ecs register-task-definition --cli-input-json file:///tmp/td.json --query 'taskDefinition.taskDefinitionArn' --output text)
  echo "$service: registered $new (from ${template##*/}; running $current)"
  wanted[$service]=$new
done

echo "migrating"
if ! tools/ci/corp-task.sh "${wanted[awzy-corp-portal]}" deploy-corp-migrate migrate; then
  echo "::error::The migration failed; nothing was deployed." >&2
  exit 1
fi

# This commit's authz chart is the fast channel's now (ADR 0094); publish-chart.sh named it so.
tools/ci/corp-task.sh "${wanted[awzy-corp-portal]}" deploy-corp-channel \
  channel set fast "0.1.0-g${AWZY_CORP_PORTAL_RELEASE_TAG:0:12}" github-actions

for service in "${!wanted[@]}"; do
  aws ecs update-service --cluster "$CLUSTER" --service "$service" --task-definition "${wanted[$service]}" \
    --query 'service.serviceName' --output text > /dev/null
done

echo "waiting for the services to settle"
aws ecs wait services-stable --cluster "$CLUSTER" --services "${!wanted[@]}"

failed=0
for service in "${!wanted[@]}"; do
  running=$(aws ecs describe-services --cluster "$CLUSTER" --services "$service" --query 'services[0].taskDefinition' --output text)
  if [ "$running" = "${wanted[$service]}" ]; then
    echo "$service: deployed $AWZY_CORP_PORTAL_RELEASE_TAG"
  else
    echo "::error::$service: rolled back to $running (the new revision didn't become healthy)"
    failed=1
  fi
done
exit $failed

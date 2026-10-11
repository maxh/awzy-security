#!/usr/bin/env bash
# Makes a chart version the stable channel's (ADR 0094), after it has soaked: CI published it at
# least AWZY_CORP_PROMOTE_SOAK_HOURS ago (default 24), and so it has been the fast channel's
# since. Run by .github/workflows/promote.yml, from main, as awzy-ci-deploy.
#
#   AWZY_CORP_PROMOTE_BY=<who asked> tools/ci/promote.sh 0.1.0-g<12 hex digits>
set -euo pipefail
version=${1:?usage: $0 <chart version>}
by=${AWZY_CORP_PROMOTE_BY:?}
soak_hours=${AWZY_CORP_PROMOTE_SOAK_HOURS:-24}
[[ $version =~ ^0\.[0-9]+\.[0-9]+-g[0-9a-f]{12}$ ]] || { echo "::error::$version isn't a chart version CI publishes" >&2; exit 1; }

pushed=$(aws ecr describe-images --repository-name awzy/charts/awzy --image-ids "imageTag=$version" \
  --query 'imageDetails[0].imagePushedAt' --output text 2>/dev/null) ||
  { echo "::error::No chart $version in the release registry." >&2; exit 1; }
age_hours=$(python3 -c 'import sys, datetime; t = datetime.datetime.fromisoformat(sys.argv[1]); print(int((datetime.datetime.now(t.tzinfo) - t).total_seconds() // 3600))' "$pushed")
if [ "$age_hours" -lt "$soak_hours" ]; then
  echo "::error::$version was published $age_hours hours ago; it soaks in fast for $soak_hours first." >&2
  exit 1
fi

tools/ci/corp-task.sh current promote-stable channel set stable "$version" "$by"
echo "stable is $version (asked by $by, published $age_hours hours ago)"

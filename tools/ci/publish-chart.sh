#!/usr/bin/env bash
# Part of the authz release (ADR 0053): packages deploy/helm/awzy as 0.1.0-g<sha> (appVersion = the image tag), pushes it to
# oci://$AWZY_AUTHZ_RELEASE_REGISTRY/awzy/charts, and signs it. Runs after publish-images.sh, so the chart only
# ever names images that exist.
set -euo pipefail
: "${AWZY_AUTHZ_RELEASE_REGISTRY:?}" "${AWZY_AUTHZ_RELEASE_TAG:?}" "${AWZY_AUTHZ_RELEASE_COSIGN_KEY:?}"

version="0.1.0-g${AWZY_AUTHZ_RELEASE_TAG:0:12}"
out=$(mktemp -d)
helm package deploy/helm/awzy --version "$version" --app-version "$AWZY_AUTHZ_RELEASE_TAG" --destination "$out"
aws ecr get-login-password | helm registry login --username AWS --password-stdin "$AWZY_AUTHZ_RELEASE_REGISTRY"
push=$(helm push "$out/awzy-$version.tgz" "oci://$AWZY_AUTHZ_RELEASE_REGISTRY/awzy/charts" 2>&1 | tee /dev/stderr)
digest=$(sed -n 's/^Digest: \(sha256:[0-9a-f]*\).*/\1/p' <<< "$push")
[ -n "$digest" ] || { echo "no digest in helm push output" >&2; exit 1; }
cosign sign --yes --tlog-upload=false --key "$AWZY_AUTHZ_RELEASE_COSIGN_KEY" "$AWZY_AUTHZ_RELEASE_REGISTRY/awzy/charts/awzy@$digest" </dev/null
echo "chart_version=$version" >> "${GITHUB_OUTPUT:-/dev/null}"

#!/usr/bin/env bash
# Part of the authz release (ADR 0053): packages deploy/helm/awzy as 0.1.0-g<sha> (appVersion = the image tag), pushes it to
# oci://$AWZY_AUTHZ_RELEASE_REGISTRY/awzy/charts, and signs it. Runs after publish-images.sh, so the chart only
# ever names images that exist. The chart carries those images' digests (image-digests.yaml, from
# publish-images.sh): Kyverno writes a digest into any image without one, which Argo CD would
# otherwise see as drift from the chart on every sync.
set -euo pipefail
: "${AWZY_AUTHZ_RELEASE_REGISTRY:?}" "${AWZY_AUTHZ_RELEASE_TAG:?}" "${AWZY_AUTHZ_RELEASE_COSIGN_KEY:?}"
: "${AWZY_AUTHZ_RELEASE_DIGESTS:?}"

version="0.1.0-g${AWZY_AUTHZ_RELEASE_TAG:0:12}"
out=$(mktemp -d)
cp -R deploy/helm/awzy "$out/awzy"
cp "$AWZY_AUTHZ_RELEASE_DIGESTS" "$out/awzy/image-digests.yaml"
helm package "$out/awzy" --version "$version" --app-version "$AWZY_AUTHZ_RELEASE_TAG" --destination "$out"
aws ecr get-login-password | helm registry login --username AWS --password-stdin "$AWZY_AUTHZ_RELEASE_REGISTRY"
push=$(helm push "$out/awzy-$version.tgz" "oci://$AWZY_AUTHZ_RELEASE_REGISTRY/awzy/charts" 2>&1 | tee /dev/stderr)
digest=$(sed -n 's/^Digest: \(sha256:[0-9a-f]*\).*/\1/p' <<< "$push")
[ -n "$digest" ] || { echo "no digest in helm push output" >&2; exit 1; }
cosign sign --yes --tlog-upload=false --key "$AWZY_AUTHZ_RELEASE_COSIGN_KEY" "$AWZY_AUTHZ_RELEASE_REGISTRY/awzy/charts/awzy@$digest" </dev/null
echo "chart_version=$version" >> "${GITHUB_OUTPUT:-/dev/null}"

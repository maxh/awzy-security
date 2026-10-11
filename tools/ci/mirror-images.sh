#!/usr/bin/env bash
# Mirrors deploy/release/third-party-images.txt into the release registry, signs each image with
# the release key (ADR 0057), and attaches its SBOM (syft) as a signed attestation (ADR 0093).
# Run by CI's publish step, after publish-images.sh authz, with the same registry and key. Images
# already mirrored, signed, and attested are left alone, so it's cheap to run on every publish;
# a step that's missing is done.
#
#   AWZY_AUTHZ_RELEASE_REGISTRY=… AWZY_AUTHZ_RELEASE_COSIGN_KEY=awskms:///alias/awzy-cosign \
#     tools/ci/mirror-images.sh
#   tools/ci/mirror-images.sh --check   # the chart uses only listed images
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
list=deploy/release/third-party-images.txt
entries=$(grep -v '^\s*\(#\|$\)' "$list")

if [ "${1:-}" = "--check" ]; then
  # Every <name>:<tag>@<digest> the chart names is in the list with that digest (bootstrap
  # reads the list itself), and no image there comes from anywhere but the release registry.
  status=0
  refs=$(grep -rhoE '"ref" "[^"]+"' deploy/helm/awzy/templates | sed -E 's/^"ref" "(.*)"$/\1/' | sort -u)
  [ -n "$refs" ] || { echo "found no third-party image refs in the chart" >&2; status=1; }
  for ref in $refs; do
    target=${ref%%@*} digest=${ref#*@}
    if [ "$target" = "$ref" ]; then
      echo "no digest on $ref; write <name>:<tag>@<digest>" >&2; status=1
    elif ! grep -qE "^$target[[:space:]]+[^[:space:]]+@$digest\$" <<< "$entries"; then
      echo "not in $list with that digest: $ref" >&2; status=1
    fi
  done
  if grep -rnE '^\s*image: *"?[a-z0-9.-]+[/:]' deploy/helm/awzy/templates | grep -v 'include "awzy\.'; then
    echo "images above don't come from the release registry; mirror them ($list)" >&2
    status=1
  fi
  exit $status
fi

registry="${AWZY_AUTHZ_RELEASE_REGISTRY:?}" key="${AWZY_AUTHZ_RELEASE_COSIGN_KEY:?}"
while read -r target source; do
  name=${target%%:*} tag=${target#*:} digest=${source##*@}
  repo="$registry/awzy/third-party/$name"
  current=$(crane digest "$repo:$tag" 2>/dev/null || true)
  if [ -z "$current" ]; then
    crane copy "$source" "$repo:$tag"
  elif [ "$current" != "$digest" ]; then
    echo "::error::$repo:$tag is $current, not $digest. Tags are immutable; give the new digest a new tag." >&2
    exit 1
  fi
  if cosign verify --key deploy/release/cosign.pub --insecure-ignore-tlog "$repo@$digest" >/dev/null 2>&1; then
    echo "$repo:$tag already mirrored and signed"
  else
    cosign sign --yes --tlog-upload=false --key "$key" "$repo@$digest"
    echo "$repo:$tag mirrored and signed"
  fi
  if cosign verify-attestation --type spdxjson --key deploy/release/cosign.pub --insecure-ignore-tlog \
    "$repo@$digest" >/dev/null 2>&1; then
    echo "$repo:$tag already has its SBOM"
  else
    sbom=$(mktemp)
    syft scan -q "registry:$repo@$digest" -o "spdx-json=$sbom"
    cosign attest --yes --tlog-upload=false --key "$key" --type spdxjson --predicate "$sbom" "$repo@$digest"
    rm -f "$sbom"
    echo "$repo:$tag SBOM attached"
  fi
done <<< "$entries"

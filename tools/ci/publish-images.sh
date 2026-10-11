#!/usr/bin/env bash
# Pushes one release's images to <registry>/awzy/<binary>:<tag> and signs each digest with
# cosign (ADR 0053). Run by .github/workflows/ci.yml on pushes to main, after AWS credentials for
# the publish role are in the environment.
#
#   authz:        everything an install runs: authz/, the client portal, and the demo's dev IdP
#                 (AWZY_AUTHZ_RELEASE_REGISTRY, _TAG, _COSIGN_KEY)
#   corp-portal:  the vendor's own services in portals/corp/: the corp portal and the registry
#                 proxy (AWZY_CORP_PORTAL_RELEASE_REGISTRY, _TAG, _COSIGN_KEY)
#
#   AWZY_AUTHZ_RELEASE_REGISTRY=123456789012.dkr.ecr.us-east-2.amazonaws.com AWZY_AUTHZ_RELEASE_TAG=<sha> \
#   AWZY_AUTHZ_RELEASE_COSIGN_KEY=awskms:///alias/awzy-cosign tools/ci/publish-images.sh authz
set -euo pipefail
case "${1:-}" in
  authz)
    scope='//authz/... + //portals/client/... + //tools/dev-idp/...'
    registry="${AWZY_AUTHZ_RELEASE_REGISTRY:?}" tag="${AWZY_AUTHZ_RELEASE_TAG:?}" key="${AWZY_AUTHZ_RELEASE_COSIGN_KEY:?}"
    digests="${AWZY_AUTHZ_RELEASE_DIGESTS:-}"
    ;;
  corp-portal)
    scope='//portals/corp/...'
    registry="${AWZY_CORP_PORTAL_RELEASE_REGISTRY:?}" tag="${AWZY_CORP_PORTAL_RELEASE_TAG:?}"
    key="${AWZY_CORP_PORTAL_RELEASE_COSIGN_KEY:?}"
    digests=""
    ;;
  *) echo "usage: $0 authz|corp-portal" >&2; exit 2 ;;
esac

# image_push targets in the release's scope, each with the default repository the macro gives it,
# awzy/<binary>, as its name under $registry. One query for all of them: each Bazel command
# costs a second or more.
targets=$(bazel query --output=streamed_jsonproto \
  "attr(name, \"^image_push\$\", $scope)" </dev/null |
  python3 -c '
import json, sys
for line in sys.stdin:
    rule = json.loads(line)["rule"]
    repo = next((a["stringValue"] for a in rule["attribute"] if a["name"] == "repository"), "")
    if not repo:
        sys.exit("no repository attribute on " + rule["name"])
    print(rule["name"].removesuffix(":image_push"), repo)
')
[ -n "$targets" ] || { echo "no image_push targets found" >&2; exit 1; }

# Every repository must exist before anything is pushed (deploy/terraform/corp/build, images).
# Asked by name: the publish role may describe only awzy/* repositories, not list them all.
missing=""
while read -r pkg repo; do
  if ! err=$(aws ecr describe-repositories --repository-names "$repo" 2>&1 >/dev/null); then
    case "$err" in
      *RepositoryNotFoundException*) missing+=" $repo" ;;
      *) echo "::error::Checking ECR repository $repo failed: $err" >&2; exit 1 ;;
    esac
  fi
done <<< "$targets"
if [ -n "$missing" ]; then
  echo "::error::No ECR repository for:$missing. Add them to local.images in deploy/terraform/corp/build/main.tf and apply." >&2
  exit 1
fi

# Build every image in one command, optimized (--config=release), as top-level targets so their
# files are local even with a remote cache, and find each one's OCI layout.
pkgs=$(cut -d' ' -f1 <<< "$targets")
bazel build --config=ci --config=release $(sed 's/$/:image_index/' <<< "$pkgs") $(sed 's/$/:image_push/' <<< "$pkgs") </dev/null
layouts=$(bazel cquery --config=ci --config=release "set($(sed 's/$/:image_index/' <<< "$pkgs" | tr '\n' ' '))" \
  --output=starlark --starlark:expr='str(target.label).removeprefix("@@") + " " + target.files.to_list()[0].path' \
  2>/dev/null </dev/null)

# Bazel runs one command at a time, so write each push as a script (already built, about a
# second each), then push and sign in parallel.
scripts=$(mktemp -d)
trap 'rm -rf "$scripts"' EXIT
while read -r pkg repo; do
  name=$(tr '/:' '__' <<< "${pkg#//}")
  bazel run --config=ci --config=release --script_path="$scripts/$name.sh" "$pkg:image_push" </dev/null >&2
  layout=$(awk -v l="$pkg:image_index" '$1 == l { print $2 }' <<< "$layouts")
  digest=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1] + "/index.json"))["manifests"][0]["digest"])' "$layout")
  echo "$name $repo $digest"
done <<< "$targets" > "$scripts/plan"

# No upload to the public Rekor log: installs verify against the KMS public key only.
push_and_sign() {
  "$scripts/$1.sh" --repository "$registry/$2" --tag "$tag" &&
    cosign sign --yes --tlog-upload=false --key "$key" "$registry/$2@$3"
}
export -f push_and_sign
export scripts registry tag key
failed=0
xargs -P 6 -L 1 bash -c 'push_and_sign "$0" "$@" > "$scripts/$0.log" 2>&1 || { echo "$0" >> "$scripts/failed"; }' \
  < "$scripts/plan" || failed=1
while read -r name repo digest; do
  echo "::group::$registry/$repo:$tag"; cat "$scripts/$name.log"; echo "::endgroup::"
done < "$scripts/plan"
if [ "$failed" = 1 ] || [ -s "$scripts/failed" ]; then
  echo "::error::Failed to push or sign: $(tr '\n' ' ' < "$scripts/failed" 2>/dev/null)" >&2
  exit 1
fi
# <binary>: <digest>, for publish-chart.sh to pin each image the chart names.
if [ -n "$digests" ]; then
  awk '{ sub(/^awzy\//, "", $2); print $2 ": " $3 }' "$scripts/plan" > "$digests"
fi
echo "pushed and signed $(wc -l < "$scripts/plan" | tr -d ' ') images"

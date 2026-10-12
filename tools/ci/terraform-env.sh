# Sourced by the corp Terraform scripts in CI (terraform-corp.sh, terraform-drift.sh): writes the
# corp deployment.env from DEPLOYMENT_ENV and loads it, and points the profiles the modules name at
# this job's AWS credentials. Plans hold account IDs and these logs are public, so `redact` hides
# them and the scripts print resource addresses only.

env_file=deploy/terraform/corp/deployment.env
printf '%s\n' "${DEPLOYMENT_ENV:?DEPLOYMENT_ENV is not set}" > "$env_file"
set -a
# shellcheck disable=SC1090
source "$env_file"
set +a

mkdir -p ~/.aws
for profile in "${TF_VAR_build_profile:-}" "${TF_VAR_management_profile:-}"; do
  [ -n "$profile" ] || continue
  printf '[%s]\naws_access_key_id = %s\naws_secret_access_key = %s\naws_session_token = %s\n' \
    "$profile" "$AWS_ACCESS_KEY_ID" "$AWS_SECRET_ACCESS_KEY" "$AWS_SESSION_TOKEN" >> ~/.aws/credentials
done

# corp-portal's Cloudflare provider reads its token from a file. A plan without a refresh reads
# nothing from Cloudflare, so it only needs one to start.
cf_token=$(mktemp)
if [ -n "${CLOUDFLARE_TOKEN:-}" ]; then printf '%s' "$CLOUDFLARE_TOKEN" > "$cf_token"; else printf '%040d' 0 > "$cf_token"; fi
export TF_VAR_cloudflare_token_file="$cf_token"

# Twelve-digit numbers are account IDs; never print one.
redact() { sed -E 's/[0-9]{12}/<account>/g'; }

# What a saved plan would change, one `<actions>\t<address>` line each.
plan_changes() { # <dir> <plan file>
  (cd "$1" && tofu show -json "$2") |
    jq -r '.resource_changes[] | select(.change.actions != ["no-op"] and .change.actions != ["read"]) | "\(.change.actions | join("+"))\t\(.address)"' |
    redact
}

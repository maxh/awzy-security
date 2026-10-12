# Sourced by the Terraform scripts in CI (terraform-ci.sh, terraform-drift.sh): writes a
# deployment's deployment.env (into $env_dir) from DEPLOYMENT_ENV and loads it, and points the
# profiles its modules name at this job's AWS credentials: the management account's at
# MANAGEMENT_AWS_* when the job has a role there of its own, every other at AWS_*. Plans hold
# account IDs and these logs are public, so `redact` hides them and the scripts print resource
# addresses only.

env_file=${env_dir:?env_dir is not set}/deployment.env
printf '%s\n' "${DEPLOYMENT_ENV:?DEPLOYMENT_ENV is not set}" > "$env_file"
set -a
# shellcheck disable=SC1090
source "$env_file"
set +a

mkdir -p ~/.aws
write_profile() { # <name> <key id> <secret> <session token>
  [ -n "$1" ] || return 0
  printf '[%s]\naws_access_key_id = %s\naws_secret_access_key = %s\naws_session_token = %s\n' "$@" >> ~/.aws/credentials
}
for profile in "${TF_VAR_build_profile:-}" "${TF_VAR_install_profile:-}"; do
  write_profile "$profile" "$AWS_ACCESS_KEY_ID" "$AWS_SECRET_ACCESS_KEY" "$AWS_SESSION_TOKEN"
done
write_profile "${TF_VAR_management_profile:-}" "${MANAGEMENT_AWS_ACCESS_KEY_ID:-$AWS_ACCESS_KEY_ID}" \
  "${MANAGEMENT_AWS_SECRET_ACCESS_KEY:-$AWS_SECRET_ACCESS_KEY}" "${MANAGEMENT_AWS_SESSION_TOKEN:-$AWS_SESSION_TOKEN}"

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

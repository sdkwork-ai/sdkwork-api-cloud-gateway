#!/usr/bin/env bash
# prepare-envs.sh — recreate per-environment env files from the bundled
# examples and assign per-environment snowflake node ids (required when
# several environments share one host; see
# docs/guides/operator/docker-install.md §6).
#
# Never overwrites an existing env/<environment>.env unless --force is given,
# so re-running after a bundle upgrade keeps operator edits.
#
# Usage:
#   bash prepare-envs.sh [--force] [--dev-secrets-for-production]
#
# Options:
#   --force                       overwrite existing env/<environment>.env files
#   --dev-secrets-for-production  fill the 6 production Cloud Router secrets
#                                 with the development examples' dev-only
#                                 defaults. LOCAL VERIFICATION ONLY — never
#                                 use for a real production deployment (see
#                                 docker-install.md §4.1 for real secrets).
set -euo pipefail
cd "$(dirname "$0")"

force=0
dev_secrets=0
for arg in "$@"; do
  case "$arg" in
    --force) force=1 ;;
    --dev-secrets-for-production) dev_secrets=1 ;;
    *) echo "unknown option: $arg (see header for usage)" >&2; exit 1 ;;
  esac
done

# Suggested distinct node ids for co-located fleets (docs §6).
declare -A NODE_IDS=( [development]=21 [test]=22 [staging]=23 [production]=24 [demo]=25 )

shopt -s nullglob
examples=(env/*.env.example)
if [[ ${#examples[@]} -eq 0 ]]; then
  echo "no env/*.env.example templates found next to this script" >&2
  exit 1
fi
for example in "${examples[@]}"; do
  environment=$(basename "$example" .env.example)
  target="env/${environment}.env"
  if [[ -f "$target" && "$force" -ne 1 ]]; then
    echo "keep existing ${target} (use --force to overwrite)"
    continue
  fi
  cp "$example" "$target"
  node_id=${NODE_IDS[$environment]:-}
  if [[ -n "$node_id" ]]; then
    sed -i "s/^GATEWAY_IM_ID_NODE_ID=.*/GATEWAY_IM_ID_NODE_ID=${node_id}/" "$target"
  fi
  echo "created ${target} (node id ${node_id:-example default})"
done

if [[ "$dev_secrets" -eq 1 ]]; then
  for key in \
    GATEWAY_CLOUDROUTER_API_KEY_PEPPER \
    GATEWAY_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING \
    GATEWAY_CLOUDROUTER_TRUSTED_SUBJECT_SECRET \
    GATEWAY_CLOUDROUTER_APP_SESSION_SECRET \
    GATEWAY_CLOUDROUTER_PAYMENT_WEBHOOK_SECRET \
    GATEWAY_CLOUDROUTER_INTERNAL_GATEWAY_SIGNING_SECRET; do
    value=$(grep -E "^${key}=" env/development.env | head -1 | cut -d= -f2-)
    [[ -n "$value" ]] || { echo "missing ${key} in env/development.env" >&2; exit 1; }
    value=${value//&/\\&}
    sed -i "s|^${key}=.*|${key}=${value}|" env/production.env
  done
  echo "production Cloud Router secrets filled from development defaults (DEV-ONLY; local verification only)"
fi

leftover=$(grep -l "<CHANGE_ME>" env/*.env 2>/dev/null || true)
if [[ -n "$leftover" ]]; then
  echo
  echo "NOTE: <CHANGE_ME> placeholders remain in: ${leftover}"
  echo "      smoke tests work, but replace them before anything real"
  echo "      (one-liner generators: docker-install.md §4.1)."
fi

echo
echo "next: bash deploy.sh --environment <env> [--embedded] [--replicas N]"
echo "then: curl --noproxy '*' http://127.0.0.1:<port>/healthz"

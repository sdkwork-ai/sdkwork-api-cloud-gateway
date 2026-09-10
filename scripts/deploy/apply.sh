#!/usr/bin/env bash
# scripts/deploy/apply.sh
#
# Canonical, idempotent deployment entrypoint for sdkwork-api-cloud-gateway.
#
# What it does (and re-running is safe):
#   1. prepare host-native PostgreSQL + Redis when requested (external posture)
#   2. build the container image (unless --skip-build and the image exists)
#   3. deploy each environment against external PostgreSQL/Redis
#   4. health-check every environment on 127.0.0.1:<port>/healthz
#
# Public reverse proxy is owned by the independent sdkwork-webserver product.
# This script does NOT generate or install nginx site mappings.
#
# Environments run as isolated compose projects with distinct host ports (3910-3914)
# and distinct sdkwork-specs databases, so dev/test/staging/prod never collide.
#
# Usage:
#   sudo bash scripts/deploy/apply.sh                 # full cycle, all environments
#   sudo bash scripts/deploy/apply.sh --skip-build     # reuse sdkwork-api-cloud-gateway:local
#   sudo bash scripts/deploy/apply.sh --skip-host      # assume PostgreSQL/Redis already run
#   sudo bash scripts/deploy/apply.sh --env development test
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=lib/environments.sh
source "$SCRIPT_DIR/lib/environments.sh"

repo_root=$(deploy_repo_root)
SKIP_BUILD=false
SKIP_HOST=false
SELECTED=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=true ;;
    --skip-host)  SKIP_HOST=true ;;
    --no-nginx|--skip-nginx)
      # Accepted for backward compatibility; nginx is never installed here.
      log_warn "ignoring $1 — reverse proxy is owned by sdkwork-webserver"
      ;;
    --env)        shift; while [[ $# -gt 0 && "$1" != -* ]]; do SELECTED+=("$1"); shift; done; continue ;;
    -h|--help)    sed -n '2,24p' "$0"; exit 0 ;;
    -*)           die "unknown option: $1" ;;
    *)            SELECTED+=("$1") ;;
  esac
  shift
done
[[ ${#SELECTED[@]} -eq 0 ]] && SELECTED=($SDKWORK_DEPLOY_ENVS)

log_step "sdkwork-api-cloud-gateway deployment — envs: ${SELECTED[*]}"
require_docker

if [[ "$SKIP_HOST" == true ]]; then
  log_info "0/4 --skip-host: assuming PostgreSQL/Redis already available"
else
  os=$(detect_os)
  if [[ "$os" != "linux" ]]; then
    log_warn "0/4 host service prep is Linux-only; skipped on $os (provide external DB/Redis)"
  else
    log_step "0/4 preparing host PostgreSQL + Redis (idempotent)"
    run_root bash "$SCRIPT_DIR/prepare-host.sh"
  fi
fi

# Ensure the PostgreSQL CA bundle path always resolves for the optional mount.
ca_bundle="$repo_root/docker/ca/postgres-ca-bundle.crt"
mkdir -p "$(dirname "$ca_bundle")"
[[ -f "$ca_bundle" ]] || : > "$ca_bundle"

if [[ "$SKIP_BUILD" == true ]] && docker image inspect sdkwork-api-cloud-gateway:local >/dev/null 2>&1; then
  log_ok "skipping image build (--skip-build and image present)"
else
  log_step "1/4 building container image sdkwork-api-cloud-gateway:local"
  (cd "$repo_root" && pnpm build:container)
fi

log_step "2/4 deploying environments (external PostgreSQL/Redis, no nginx)"
for env in "${SELECTED[@]}"; do
  bash "$repo_root/scripts/deploy-docker-environment.sh" "$env" --validate
done

log_step "3/4 health checks"
for env in "${SELECTED[@]}"; do
  port=$(env_host_port "$env")
  if curl -fsS --max-time 5 "http://127.0.0.1:${port}/healthz" >/dev/null 2>&1; then
    log_ok "$env  http://127.0.0.1:$port/healthz  OK"
  else
    log_warn "$env  http://127.0.0.1:$port/healthz  not ready (still starting / migrations running)"
  fi
done

log_step "DONE"
log_info "Public reverse proxy: configure sdkwork-webserver (independent Docker deploy)"
log_info "Direct health: curl -fsS http://127.0.0.1:3910/healthz"
log_info "Re-run: bash scripts/deploy/apply.sh"

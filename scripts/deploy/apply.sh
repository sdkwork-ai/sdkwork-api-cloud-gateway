#!/usr/bin/env bash
# scripts/deploy/apply.sh
#
# Canonical, idempotent deployment entrypoint for sdkwork-api-cloud-gateway.
#
# What it does (and re-running is safe):
#   1. generate nginx site configs for every selected environment
#   2. generate self-signed TLS certs (skips if still valid)
#   3. build the container image (unless --skip-build and the image exists)
#   4. deploy each environment in EXTERNAL-dependency mode
#      (PostgreSQL + Redis come from outside the composition — host services or
#       a remote managed database; see docker-compose.external.yml)
#   5. install TLS certs + nginx sites on the Ubuntu host (port 80/443 ingress)
#   6. health-check every environment on 127.0.0.1:<port>/healthz
#
# Environments run as isolated compose projects with distinct host ports (3910-3913)
# and distinct sdkwork-specs databases, so dev/test/staging/prod never collide.
#
# Usage:
#   sudo bash scripts/deploy/apply.sh                 # full cycle, all environments
#   sudo bash scripts/deploy/apply.sh --skip-build     # reuse sdkwork-api-cloud-gateway:local
#   sudo bash scripts/deploy/apply.sh --skip-host      # assume PostgreSQL/Redis already run
#   sudo bash scripts/deploy/apply.sh --env development test
#   sudo bash scripts/deploy/apply.sh --no-nginx        # containers only (no host nginx)
#
# On a non-Linux host the nginx/cert steps are skipped automatically (nginx is a
# server-side concern); the gateway stays reachable on 127.0.0.1:<port>.
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=lib/environments.sh
source "$SCRIPT_DIR/lib/environments.sh"

repo_root=$(deploy_repo_root)
SKIP_BUILD=false
SKIP_HOST=false
NO_NGINX=false
SELECTED=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=true ;;
    --skip-host)  SKIP_HOST=true ;;
    --no-nginx)   NO_NGINX=true ;;
    --env)        shift; while [[ $# -gt 0 && "$1" != -* ]]; do SELECTED+=("$1"); shift; done; continue ;;
    -h|--help)    sed -n '2,30p' "$0"; exit 0 ;;
    -*)           die "unknown option: $1" ;;
    *)            SELECTED+=("$1") ;;
  esac
  shift
done
[[ ${#SELECTED[@]} -eq 0 ]] && SELECTED=($SDKWORK_DEPLOY_ENVS)

log_step "sdkwork-api-cloud-gateway deployment — envs: ${SELECTED[*]}"
require_docker

# 0/7 — prepare host-native PostgreSQL + Redis (external-dependency posture).
# Skipped with --skip-host when you point the env files at a managed/cloud DB.
if [[ "$SKIP_HOST" == true ]]; then
  log_info "0/7 --skip-host: assuming PostgreSQL/Redis already available"
else
  os=$(detect_os)
  if [[ "$os" != "linux" ]]; then
    log_warn "0/7 host service prep is Linux-only; skipped on $os (provide external DB/Redis)"
  else
    log_step "0/7 preparing host PostgreSQL + Redis (idempotent)"
    run_root bash "$SCRIPT_DIR/prepare-host.sh"
  fi
fi

log_step "1/7 generating nginx site configs"
bash "$SCRIPT_DIR/generate-nginx-conf.sh" "${SELECTED[@]}"

log_step "2/7 generating TLS certificates"
bash "$SCRIPT_DIR/generate-certs.sh" "${SELECTED[@]}"

if [[ "$SKIP_BUILD" == true ]] && docker image inspect sdkwork-api-cloud-gateway:local >/dev/null 2>&1; then
  log_ok "skipping image build (--skip-build and image present)"
else
  log_step "3/7 building container image sdkwork-api-cloud-gateway:local"
  (cd "$repo_root" && pnpm build:container)
fi

# Ensure the PostgreSQL CA bundle exists so the external compose trust mount
# always resolves (dev/test use sslmode=disable and ignore it; prod needs it).
ca_bundle="$repo_root/docker/ca/postgres-ca-bundle.crt"
mkdir -p "$(dirname "$ca_bundle")"
[[ -f "$ca_bundle" ]] || : > "$ca_bundle"

log_step "4/7 deploying environments (external PostgreSQL/Redis)"
for env in "${SELECTED[@]}"; do
  bash "$repo_root/scripts/deploy-docker-environment.sh" "$env" --validate
done

if [[ "$NO_NGINX" == true ]]; then
  log_warn "5/7 --no-nginx: skipped host TLS + nginx install"
  log_info "gateways are reachable on 127.0.0.1:<port>/healthz"
else
  os=$(detect_os)
  if [[ "$os" != "linux" ]]; then
    log_warn "5/7 nginx host install is Linux-only; skipped on $os (use 127.0.0.1:<port>)"
  else
    log_step "5/7 installing TLS certs + nginx sites (requires root)"
    bash "$SCRIPT_DIR/install-certs.sh" "${SELECTED[@]}"
    run_root bash "$repo_root/scripts/install-nginx-sites.sh" "${SELECTED[@]}"
    log_ok "port 80/443 ingress active under /etc/nginx/sites-enabled/sdkwork/"
  fi
fi

log_step "6/7 health checks"
for env in "${SELECTED[@]}"; do
  port=$(env_host_port "$env")
  if curl -fsS --max-time 5 "http://127.0.0.1:${port}/healthz" >/dev/null 2>&1; then
    log_ok "$env  http://127.0.0.1:$port/healthz  OK"
  else
    log_warn "$env  http://127.0.0.1:$port/healthz  not ready (still starting / migrations running)"
  fi
  if [[ "$NO_NGINX" == false ]] && command -v nginx >/dev/null 2>&1; then
    for dom in $(env_server_names "$env"); do
      if curl -fsS --max-time 5 "https://$dom/healthz" >/dev/null 2>&1; then
        log_ok "$env  https://$dom/healthz  OK"
      else
        log_warn "$env  https://$dom/healthz  not ready (nginx/TLS/cert issue)"
      fi
    done
  fi
done

log_step "DONE"
log_info "Windows hosts file: powershell -ExecutionPolicy Bypass -File scripts/bind-windows-hosts.ps1"
log_info "Re-run any time: sudo bash scripts/deploy/apply.sh"

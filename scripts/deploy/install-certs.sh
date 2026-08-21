#!/usr/bin/env bash
# scripts/deploy/install-certs.sh
#
# Install the generated self-signed certificates into the Ubuntu host trust path
# that nginx reads: /etc/ssl/sdkwork/api-gateway/<env>/{fullchain,privkey}.pem
#
# Requires root (use sudo, or set SDKWORK_SUDO_PASSWORD for non-interactive WSL/CI).
#
# Usage:
#   sudo bash scripts/deploy/install-certs.sh            # all environments
#   sudo bash scripts/deploy/install-certs.sh production
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=lib/environments.sh
source "$SCRIPT_DIR/lib/environments.sh"

main() {
  local targets
  targets=($(parse_env_selection "$@"))
  log_step "installing TLS certificates: ${targets[*]}"
  for env in "${targets[@]}"; do
    local src dst
    src=$(cert_repo_dir "$env")
    dst=$(cert_host_dir "$env")
    [[ -f "$src/fullchain.pem" ]] || die "cert not generated for $env; run 'bash scripts/deploy/generate-certs.sh $env' first"
    run_root install -d -m 0755 "$dst"
    run_root install -m 0644 "$src/fullchain.pem" "$dst/fullchain.pem"
    run_root install -m 0600 "$src/privkey.pem"  "$dst/privkey.pem"
    log_ok "installed $env certs -> $dst"
  done
}

main "$@"

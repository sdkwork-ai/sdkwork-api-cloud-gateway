#!/usr/bin/env bash
# scripts/deploy/generate-certs.sh
#
# Generate multi-SAN self-signed TLS certificates for each environment (idempotent).
# Output: docker/certs/<env>/{fullchain.pem,privkey.pem}  (gitignored — local dev only).
#
# The SAN list covers every public domain for that environment, so a single cert
# secures api-dev.sdkwork.com / api-dev.birdcoder.com / api-dev.dtupay.com etc.
#
# For real production use Let's Encrypt (ACME) instead — the gateway already carries
# ACME profile config — but self-signed keeps local/offline bring-up zero-dependency.
#
# Usage:
#   bash scripts/deploy/generate-certs.sh          # all environments
#   bash scripts/deploy/generate-certs.sh production
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=lib/environments.sh
source "$SCRIPT_DIR/lib/environments.sh"

require_cmd openssl

gen_one() {
  local env=$1 dir cn san
  dir=$(cert_repo_dir "$env")
  cn=$(env_canonical_host "$env")
  # server names -> DNS:host1,DNS:host2,...
  san=$(env_server_names "$env" | sed 's/ /,DNS:/g; s/^/DNS:/')

  mkdir -p "$dir"

  if [[ -f "$dir/fullchain.pem" ]] && openssl x509 -checkend 2592000 -noout -in "$dir/fullchain.pem" 2>/dev/null; then
    log_ok "cert for $env ($cn) still valid >30d; skipping"
    return 0
  fi

  log_step "generating self-signed cert for $env ($san)"
  openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout "$dir/privkey.pem" \
    -out "$dir/fullchain.pem" \
    -days 825 \
    -subj "/CN=$cn" \
    -addext "subjectAltName=$san" \
    -addext "basicConstraints=CA:FALSE" \
    -addext "keyUsage=digitalSignature,keyEncipherment" \
    -addext "extendedKeyUsage=serverAuth" \
    2>/dev/null

  chmod 600 "$dir/privkey.pem"
  chmod 644 "$dir/fullchain.pem"
  log_ok "wrote $dir/{fullchain,privkey}.pem"
}

main() {
  local targets
  targets=($(parse_env_selection "$@"))
  log_step "generating TLS certificates: ${targets[*]}"
  for env in "${targets[@]}"; do gen_one "$env"; done
  log_info "install to host with 'bash scripts/deploy/install-certs.sh'"
}

main "$@"

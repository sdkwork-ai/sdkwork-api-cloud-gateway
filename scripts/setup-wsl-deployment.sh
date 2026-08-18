#!/bin/bash
# End-to-end WSL deployment: sync nginx, redeploy all gateway stacks, verify.
#
# Prerequisites: Docker, PostgreSQL (5432), Redis (6380), nginx on WSL Ubuntu.
# Run from repository root inside WSL:
#   scripts/setup-wsl-deployment.sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

chmod +x scripts/deploy-docker-environment.sh \
  scripts/install-nginx-sites.sh \
  scripts/provision-external-databases.sh \
  scripts/package-docker-environments.sh

echo "==> validating deployment matrix"
node scripts/validate-docker-deployment.mjs --matrix

if ! docker image inspect sdkwork-api-cloud-gateway:local >/dev/null 2>&1; then
  echo "==> building container image"
  pnpm build:container
fi

echo "==> redeploying all environments (external PostgreSQL/Redis)"
scripts/deploy-docker-environment.sh all

echo "==> installing nginx sites (requires sudo)"
if [ "$(id -u)" -eq 0 ]; then
  scripts/install-nginx-sites.sh all
elif sudo -n true 2>/dev/null; then
  sudo scripts/install-nginx-sites.sh all
else
  echo "  skipped: sudo password required; nginx sites may already be installed"
  echo "  run manually: sudo scripts/install-nginx-sites.sh all"
fi

echo "==> health checks"
for port in 3910 3911 3912 3913; do
  curl -fsS "http://127.0.0.1:${port}/healthz" >/dev/null && echo "  port $port OK"
done

for domain in \
  api-dev.sdkwork.com \
  api-test.birdcoder.com \
  api-staging.dtupay.com; do
  curl -fsS "http://${domain}/healthz" >/dev/null && echo "  http://${domain} OK"
done

echo ""
echo "WSL deployment ready. On Windows (Admin PowerShell):"
echo "  powershell -ExecutionPolicy Bypass -File scripts/bind-windows-hosts.ps1"
echo ""
echo "SETUP_WSL_DEPLOYMENT_DONE"

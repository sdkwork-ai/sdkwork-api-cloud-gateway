#!/bin/bash
# Install nginx reverse-proxy sites for sdkwork-api-cloud-gateway containers.
#
# Non-production environments listen on port 80 and proxy to the per-environment
# host port (3910 development, 3911 test, 3912 staging). Production also
# terminates TLS on 443 (see docker/nginx/api.sdkwork.com.conf).
#
# Usage:
#   sudo scripts/install-nginx-sites.sh all
#   sudo scripts/install-nginx-sites.sh development test production
set -eu

usage() {
  cat <<'EOF'
Usage: install-nginx-sites.sh <environment|all> [environment ...]

Environments: development, test, staging, production, all

Installs docker/nginx/*.conf into /etc/nginx/sites-enabled/sdkwork/ and reloads nginx.
EOF
}

if [ "$(id -u)" -ne 0 ]; then
  echo "nginx installation requires root; rerun with sudo" >&2
  exit 1
fi

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
nginx_target=/etc/nginx/sites-enabled/sdkwork

domain_for() {
  case "$1" in
    development) echo api-dev.sdkwork.com ;;
    test) echo api-test.sdkwork.com ;;
    staging) echo api-staging.sdkwork.com ;;
    production) echo api.sdkwork.com ;;
    *) return 1 ;;
  esac
}

install_one() {
  env_name=$1
  domain=$(domain_for "$env_name") || {
    echo "unsupported environment: $env_name" >&2
    exit 2
  }
  source_conf="$repo_root/docker/nginx/$domain.conf"
  if [ ! -f "$source_conf" ]; then
    echo "missing nginx config: $source_conf" >&2
    exit 1
  fi
  install -d -m 0755 "$nginx_target"
  install -m 0644 "$source_conf" "$nginx_target/$domain.conf"
  echo "installed $domain.conf"
}

targets=("$@")
if [ "${#targets[@]}" -eq 0 ]; then
  usage >&2
  exit 2
fi

if [ "${targets[0]}" = all ]; then
  targets=(development test staging production)
fi

for env_name in "${targets[@]}"; do
  install_one "$env_name"
done

if [ -d /var/www/certbot ]; then
  :
else
  install -d -m 0755 /var/www/certbot
fi

nginx -t
systemctl reload nginx
echo "nginx reloaded; port 80 sites are active under $nginx_target"

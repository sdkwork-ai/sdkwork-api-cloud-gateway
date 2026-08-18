#!/bin/bash
# Package per-environment docker deployment bundles.
#
# Each bundle is self-contained: the compose files, the environment's real
# .env (external PostgreSQL/Redis via host.docker.internal), the nginx site
# config, and an install README. Ports differ per environment (3910-3913);
# the external PostgreSQL/Redis instance is shared while database+schema
# differ per environment (sdkwork_ai_dev/test/staging/prod).
#
# Usage: scripts/package-docker-environments.sh
# Output: dist/docker-packages/<environment>/
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
dist_root="$repo_root/dist/docker-packages"
rm -rf "$dist_root"
mkdir -p "$dist_root"

for env in development test staging production; do
  target="$dist_root/$env"
  mkdir -p "$target/nginx"
  install -m 0644 "$repo_root/docker-compose.yml" "$target/docker-compose.yml"
  install -m 0644 "$repo_root/docker-compose.external.yml" "$target/docker-compose.external.yml"
  install -m 0640 "$repo_root/docker/env/$env.env" "$target/.env"
  install -m 0644 "$repo_root/docker/config/sdkwork-api-cloud-gateway.toml" "$target/sdkwork-api-cloud-gateway.toml"

  case "$env" in
    development) domain=api-dev.sdkwork.com ;;
    test) domain=api-test.sdkwork.com ;;
    staging) domain=api-staging.sdkwork.com ;;
    production) domain=api.sdkwork.com ;;
  esac
  install -m 0644 "$repo_root/docker/nginx/$domain.conf" "$target/nginx/$domain.conf"

  cat > "$target/README.md" <<EOF
# SDKWork API Cloud Gateway — $env docker deployment bundle

- Environment: $env (profile \`standalone.$env\`)
- Host port: ${env_port:-} (see .env GATEWAY_HOST_PORT)
- External dependencies: shared PostgreSQL (host.docker.internal:5432) and
  Redis (host.docker.internal:6380) — same instance across environments,
  per-environment database+schema (see GATEWAY_POSTGRES_DB/SCHEMA).
- Nginx site: nginx/$domain.conf -> /etc/nginx/sites-enabled/sdkwork/

## Deploy

    docker compose --env-file .env -p sdkwork-gateway-$env \\
      -f docker-compose.yml -f docker-compose.external.yml up -d

## Verify

    curl -fsS http://127.0.0.1:\$GATEWAY_HOST_PORT/healthz
EOF

  # README host port per environment
  case "$env" in
    development) port=3910 ;;
    test) port=3911 ;;
    staging) port=3912 ;;
    production) port=3913 ;;
  esac
  sed -i "s/^\\- Host port: .*/\\- Host port: $port (host mapping $port -> 3900 in the container)/" "$target/README.md"
  echo "packaged $env -> $target"
done

echo "DOCKER_PACKAGES_DONE: $dist_root"

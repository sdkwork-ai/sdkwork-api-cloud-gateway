#!/bin/bash
# Package per-environment docker deployment bundles.
#
# Each bundle is self-contained: compose + env (external PostgreSQL/Redis via
# host.docker.internal) + Ubuntu-aligned config.toml. Ports differ per
# environment (3910-3913). Reverse proxy is owned by sdkwork-webserver — this
# bundle does not ship nginx site mappings.
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
  mkdir -p "$target"
  install -m 0644 "$repo_root/docker-compose.yml" "$target/docker-compose.yml"
  install -m 0640 "$repo_root/docker/env/$env.env" "$target/.env"
  install -m 0644 "$repo_root/docker/config/config.toml" "$target/config.toml"

  case "$env" in
    development) port=3910 ;;
    test) port=3911 ;;
    staging) port=3912 ;;
    production) port=3913 ;;
  esac

  cat > "$target/README.md" <<EOF
# SDKWork API Cloud Gateway — $env docker deployment bundle

- Environment: $env (profile \`standalone.$env\`)
- Host port: $port (host mapping $port -> 3900 in the container)
- Config mount: \`config.toml\` -> \`/etc/sdkwork/api_gateway/config.toml\`
  (same path as Ubuntu install; APPLICATION_DEPLOY_LAYOUT_SPEC)
- External dependencies: PostgreSQL (\`host.docker.internal:5432\`) and
  Redis (\`host.docker.internal:6380\`, **no password**), per-environment
  database+schema (see GATEWAY_POSTGRES_DB/SCHEMA).
- Reverse proxy: configure the independent \`sdkwork-webserver\` deployment
  to upstream \`127.0.0.1:$port\` — this gateway bundle does not install nginx.

## Deploy

    docker compose --env-file .env -p sdkwork-gateway-$env -f docker-compose.yml up -d

Or from the repository root:

    scripts/deploy-docker-environment.sh $env --validate

## Verify

    curl -fsS http://127.0.0.1:$port/healthz
EOF

  echo "packaged $env -> $target"
done

echo "DOCKER_PACKAGES_DONE: $dist_root"

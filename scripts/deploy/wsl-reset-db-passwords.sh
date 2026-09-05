#!/bin/bash
set -euo pipefail
cd "$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)"
export PGPASSWORD="${SDKWORK_DATABASE_ADMIN_PASSWORD:-postgres_admin_pass}"

reset_role() {
  local env_file=$1
  local role
  local pw
  role=$(grep -E '^GATEWAY_POSTGRES_USER=' "$env_file" | cut -d= -f2- | tr -d '\r')
  pw=$(grep -E '^GATEWAY_POSTGRES_PASSWORD=' "$env_file" | cut -d= -f2- | tr -d '\r')
  echo "reset role=$role from $env_file"
  psql -h 127.0.0.1 -U postgres -v ON_ERROR_STOP=1 \
    -c "ALTER ROLE \"${role}\" WITH LOGIN PASSWORD '${pw}';"
  PGPASSWORD="$pw" psql -h 127.0.0.1 -U "$role" -d "$role" -c 'SELECT current_user AS ok;'
}

reset_role docker/env/development.env
reset_role docker/env/test.env
reset_role docker/env/production.env

# Prove host.docker.internal from a container that already exists.
# Note: /dev/tcp is a bash feature (the gateway image's /bin/sh is dash),
# so the probe runs under bash; PostgreSQL and the host Redis both live on
# the host system (5432 / 6379).
echo "==> probe host.docker.internal via gateway image"
docker run --rm --add-host=host.docker.internal:host-gateway \
  --entrypoint sh sdkwork-api-cloud-gateway:local -c \
  'getent hosts host.docker.internal' && \
docker run --rm --add-host=host.docker.internal:host-gateway \
  --entrypoint bash pgvector/pgvector:pg18 -c \
  '(echo >/dev/tcp/host.docker.internal/5432) && echo PG_OK; (echo >/dev/tcp/host.docker.internal/6379) && echo REDIS_OK'

#!/bin/bash
# One-shot: provision external DBs + vector + redeploy gateway envs.
set -euo pipefail
cd "$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)"

export SDKWORK_DATABASE_ADMIN_HOST=127.0.0.1
export SDKWORK_DATABASE_ADMIN_PORT=5432
export SDKWORK_DATABASE_ADMIN_USERNAME=postgres
export SDKWORK_DATABASE_ADMIN_PASSWORD="${SDKWORK_DATABASE_ADMIN_PASSWORD:-postgres_admin_pass}"
export PGPASSWORD="$SDKWORK_DATABASE_ADMIN_PASSWORD"

echo "==> admin ping"
psql -h 127.0.0.1 -U postgres -d postgres -c 'SELECT 1 AS ok;'

echo "==> provision databases"
bash scripts/provision-external-databases.sh development test production

for db in sdkwork_ai_dev sdkwork_ai_test sdkwork_ai_prod; do
  echo "==> vector + grants on $db"
  psql -h 127.0.0.1 -U postgres -d "$db" -v ON_ERROR_STOP=1 <<SQL
CREATE EXTENSION IF NOT EXISTS vector;
GRANT ALL ON SCHEMA "$db" TO "$db";
GRANT CREATE ON DATABASE "$db" TO "$db";
ALTER ROLE "$db" WITH CREATEDB;
SQL
done

echo "==> stop old stacks (ignore errors)"
for env in development test production staging; do
  docker compose --env-file "docker/env/${env}.env" -p "sdkwork-gateway-$env" \
    -f docker-compose.yml down --remove-orphans 2>/dev/null || true
done

echo "==> ensure image exists"
if ! docker image inspect sdkwork-api-cloud-gateway:local >/dev/null 2>&1; then
  pnpm build:container
fi

# Ensure empty CA bundle for mount
mkdir -p docker/ca
[[ -f docker/ca/postgres-ca-bundle.crt ]] || : > docker/ca/postgres-ca-bundle.crt

echo "==> deploy development test production"
for env in development test production; do
  bash scripts/deploy-docker-environment.sh "$env" --validate
done

echo "==> wait for health"
sleep 8
for port in 3910 3911 3913; do
  if curl -fsS --max-time 5 "http://127.0.0.1:${port}/healthz"; then
    echo "  OK :$port"
  else
    echo "  WAITING :$port"
  fi
  echo
done

echo "==> container status"
docker ps --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}' | grep -E 'gateway|NAMES' || true

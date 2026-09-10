#!/bin/bash
# Provision external PostgreSQL databases for all gateway lifecycle environments.
#
# Follows sdkwork-specs ENVIRONMENT_SPEC §7.1 workspace identities:
#   development -> sdkwork_ai_dev
#   test        -> sdkwork_ai_test
#   staging     -> sdkwork_ai_staging
#   demo        -> sdkwork_ai_demo
#   production  -> sdkwork_ai_prod
#
# Each database uses the same name for database, schema, and role. The vector
# extension is created inside the workspace schema (knowledgebase requirement).
#
# Prerequisites:
#   - psql client
#   - PostgreSQL with pgvector installed (postgresql-XX-pgvector on Ubuntu 22.04)
#   - Admin credentials via SDKWORK_DATABASE_ADMIN_* or PGPASSWORD
#
# Usage:
#   export SDKWORK_DATABASE_ADMIN_HOST=127.0.0.1
#   export SDKWORK_DATABASE_ADMIN_PORT=5432
#   export SDKWORK_DATABASE_ADMIN_USERNAME=postgres
#   export SDKWORK_DATABASE_ADMIN_PASSWORD=...
#   scripts/provision-external-databases.sh
#   scripts/provision-external-databases.sh development test
set -eu

usage() {
  cat <<'EOF'
Usage: provision-external-databases.sh [environment ...]

Environments: development, test, staging, demo, production (default: all five)

Admin connection (required):
  SDKWORK_DATABASE_ADMIN_HOST      default 127.0.0.1
  SDKWORK_DATABASE_ADMIN_PORT      default 5432
  SDKWORK_DATABASE_ADMIN_USERNAME  default postgres
  SDKWORK_DATABASE_ADMIN_PASSWORD  or PGPASSWORD
EOF
}

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
schema_sql="$repo_root/docker/postgres/external-schema.sql"

admin_host=${SDKWORK_DATABASE_ADMIN_HOST:-127.0.0.1}
admin_port=${SDKWORK_DATABASE_ADMIN_PORT:-5432}
admin_user=${SDKWORK_DATABASE_ADMIN_USERNAME:-postgres}

if [ ! -f "$schema_sql" ]; then
  echo "missing schema script: $schema_sql" >&2
  exit 1
fi

db_identity() {
  case "$1" in
    development) echo sdkwork_ai_dev ;;
    test) echo sdkwork_ai_test ;;
    staging) echo sdkwork_ai_staging ;;
    demo) echo sdkwork_ai_demo ;;
    production) echo sdkwork_ai_prod ;;
    *) return 1 ;;
  esac
}

default_password() {
  case "$1" in
    development) echo sdkworkdev123 ;;
    test|staging|demo|production) echo "" ;;
  esac
}

psql_admin() {
  psql -h "$admin_host" -p "$admin_port" -U "$admin_user" "$@"
}

provision_one() {
  env_name=$1
  db=$(db_identity "$env_name") || {
    echo "unsupported environment: $env_name" >&2
    exit 2
  }

  env_file="$repo_root/docker/env/$env_name.env"
  app_password=""
  if [ -f "$env_file" ]; then
    # tr -d '\r': a Windows-checked-out env file must not bake CR into the role password.
    app_password=$(grep -E '^GATEWAY_POSTGRES_PASSWORD=' "$env_file" | cut -d= -f2- | tr -d '\r' || true)
  fi
  if [ -z "$app_password" ]; then
    app_password=$(default_password "$env_name")
  fi
  if [ -z "$app_password" ]; then
    echo "set GATEWAY_POSTGRES_PASSWORD in $env_file or export it before provisioning $env_name" >&2
    exit 1
  fi

  echo "provisioning $env_name ($db) on $admin_host:$admin_port"

  if ! psql_admin -tAc "SELECT 1 FROM pg_database WHERE datname = '$db'" | grep -q 1; then
    psql_admin -v ON_ERROR_STOP=1 -c "CREATE DATABASE \"$db\";"
  fi

  psql_admin -d "$db" -v ON_ERROR_STOP=1 -v db="$db" -v app_user="$db" -f "$schema_sql"

  role_exists=$(psql_admin -v ON_ERROR_STOP=1 -tAc "SELECT 1 FROM pg_roles WHERE rolname = '$db'" || true)
  # psql performs :'var' literal-quoting on stdin/-f input but NOT on -c
  # strings (psql 18 verified), so the password statement is piped instead of
  # passed with -c: a value containing quotes can never break out of it.
  if [ "$role_exists" != "1" ]; then
    printf "CREATE ROLE \"%s\" LOGIN PASSWORD :'app_password';\n" "$db" \
      | psql_admin -v ON_ERROR_STOP=1 -v app_password="$app_password"
  else
    printf "ALTER ROLE \"%s\" WITH LOGIN PASSWORD :'app_password';\n" "$db" \
      | psql_admin -v ON_ERROR_STOP=1 -v app_password="$app_password"
  fi

  psql_admin -d "$db" -v ON_ERROR_STOP=1 -c "GRANT ALL PRIVILEGES ON DATABASE \"$db\" TO \"$db\";"
  psql_admin -d "$db" -v ON_ERROR_STOP=1 -c "GRANT ALL ON SCHEMA \"$db\" TO \"$db\";"
  psql_admin -d "$db" -v ON_ERROR_STOP=1 -c "ALTER DEFAULT PRIVILEGES IN SCHEMA \"$db\" GRANT ALL ON TABLES TO \"$db\";"
  psql_admin -d "$db" -v ON_ERROR_STOP=1 -c "ALTER DEFAULT PRIVILEGES IN SCHEMA \"$db\" GRANT ALL ON SEQUENCES TO \"$db\";"

  echo "ready $env_name: database=$db schema=$db user=$db"
}

targets=("$@")
if [ "${#targets[@]}" -eq 0 ]; then
  targets=(development test staging demo production)
fi

if [ "${targets[0]}" = "-h" ] || [ "${targets[0]}" = "--help" ]; then
  usage
  exit 0
fi

for env_name in "${targets[@]}"; do
  provision_one "$env_name"
done

echo "EXTERNAL_DATABASES_READY"

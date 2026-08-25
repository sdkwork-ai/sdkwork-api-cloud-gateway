#!/bin/bash
# Prepare the WSL/Ubuntu host to run the gateway's EXTERNAL PostgreSQL + Redis.
#
# The gateway is deployed in external-dependency mode (docker-compose.external.yml):
# the containers reach PostgreSQL/Redis on the docker host through
# host.docker.internal -> host-gateway. This script makes that host-native
# posture real on a fresh Ubuntu 22.04 WSL instance:
#
#   1. install postgresql + pgvector + redis-server (idempotent)
#   2. start them (systemd | service | pg_ctlcluster | redis-server --daemonize)
#   3. make PostgreSQL listen on 0.0.0.0:5432 and accept docker-bridge clients
#   4. issue a self-signed CA + server cert so GATEWAY_POSTGRES_SSL_MODE=require
#      (production env) validates; write the CA into docker/ca/postgres-ca-bundle.crt
#      so the gateway container's TLS trust mount resolves
#   5. provision the per-environment databases/roles/schemas
#      (sdkwork_ai_dev/test/staging/prod) via provision-external-databases.sh
#   6. make redis bind 0.0.0.0 (no auth — matches the dev/test env files)
#
# This is the "Ubuntu host-native services" external posture documented in
# docker-compose.external.yml. For a managed/cloud database instead, skip this
# script (apply.sh --skip-host) and point GATEWAY_POSTGRES_HOST /
# GATEWAY_CLOUDROUTER_REDIS_HOST at the remote endpoints.
#
# Usage:
#   sudo scripts/deploy/prepare-host.sh
#   sudo scripts/deploy/prepare-host.sh --no-tls        # plaintext PG (sslmode=disable)
#   sudo SDKWORK_DATABASE_ADMIN_PASSWORD=secret scripts/deploy/prepare-host.sh
set -eu

# ---- root guard -------------------------------------------------------------
if [ "$(id -u)" -ne 0 ]; then
  echo "prepare-host requires root; rerun with sudo" >&2
  exit 1
fi

NO_TLS=false
for a in "$@"; do case "$a" in --no-tls) NO_TLS=true ;; esac; done

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
ca_bundle="$repo_root/docker/ca/postgres-ca-bundle.crt"
mkdir -p "$(dirname "$ca_bundle")"

PG_ADMIN_USER=${SDKWORK_DATABASE_ADMIN_USERNAME:-postgres}
PG_ADMIN_PASS=${SDKWORK_DATABASE_ADMIN_PASSWORD:-sdkworkdev123}
PG_PORT=${SDKWORK_DATABASE_ADMIN_PORT:-5432}
REDIS_PORT=${GATEWAY_CLOUDROUTER_REDIS_PORT:-6380}

log() { printf '\033[36m[prepare-host]\033[0m %s\n' "$*"; }
ok()  { printf '\033[32m[prepare-host]\033[0m %s\n' "$*"; }

# ---- 1. install -------------------------------------------------------------
if ! command -v psql >/dev/null 2>&1 || ! command -v redis-server >/dev/null 2>&1; then
  log "installing postgresql + pgvector + redis-server"
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -y
  apt-get install -y postgresql postgresql-contrib redis-server
  # pgvector for the installed PostgreSQL major (Ubuntu 22.04 -> 14)
  pgver=$(pg_lsclusters 2>/dev/null | awk 'NR==1 && $1 ~ /^[0-9]+$/ {print $1}')
  pgver=${pgver:-14}
  apt-get install -y "postgresql-${pgver}-pgvector" || \
    apt-get install -y postgresql-14-pgvector || \
    log "pgvector package unavailable; vector extension will be skipped (knowledgebase module needs it)"
else
  pgver=$(pg_lsclusters 2>/dev/null | awk 'NR==1 && $1 ~ /^[0-9]+$/ {print $1}')
  pgver=${pgver:-14}
  ok "postgresql + redis-server already installed (cluster $pgver)"
fi

# ---- 2. start ---------------------------------------------------------------
pg_start() {
  if command -v systemctl >/dev/null 2>&1 && systemctl cat postgresql >/dev/null 2>&1; then
    systemctl start postgresql
  elif command -v service >/dev/null 2>&1; then
    service postgresql start
  else
    pg_ctlcluster "$pgver" main start || true
  fi
}
redis_start() {
  if command -v systemctl >/dev/null 2>&1 && systemctl cat redis-server >/dev/null 2>&1; then
    systemctl start redis-server
  elif command -v service >/dev/null 2>&1; then
    service redis-server start
  else
    redis-server --daemonize yes
  fi
}
pg_start
redis_start
ok "postgresql + redis-server started"

# ---- 3/4. postgresql config + TLS ------------------------------------------
pgconf_dir="/etc/postgresql/$pgver/main"
pgconf="$pgconf_dir/postgresql.conf"
pghba="$pgconf_dir/pg_hba.conf"
pgdata="/var/lib/postgresql/$pgver/main"

set_conf() {  # key value
  if grep -qE "^[[:space:]]*#?[[:space:]]*$1[[:space:]=]" "$pgconf"; then
    sed -i -E "s|^[[:space:]]*#?[[:space:]]*$1[[:space:]=]+.*|$1 = $2|" "$pgconf"
  else
    echo "$1 = $2" >> "$pgconf"
  fi
}

if ! grep -qE "^[[:space:]]*listen_addresses[[:space:]=]+'?\*'?" "$pgconf"; then
  set_conf listen_addresses "'*'"
fi
set_conf port "$PG_PORT"

if [ "$NO_TLS" = false ]; then
  cert_dir="$pgdata"
  ca_key="$cert_dir/ca.key"; ca_crt="$cert_dir/ca.crt"
  srv_key="$cert_dir/server.key"; srv_crt="$cert_dir/server.crt"; srv_csr="$cert_dir/server.csr"
  if [ ! -f "$srv_crt" ]; then
    log "issuing self-signed CA + PostgreSQL server cert"
    openssl genrsa -out "$ca_key" 2048 2>/dev/null
    openssl req -x509 -new -nodes -key "$ca_key" -sha256 -days 3650 \
      -subj "/CN=sdkwork-postgres-ca" -out "$ca_crt" 2>/dev/null
    openssl genrsa -out "$srv_key" 2048 2>/dev/null
    openssl req -new -key "$srv_key" -subj "/CN=host.docker.internal" -out "$srv_csr" 2>/dev/null
    openssl x509 -req -in "$srv_csr" -CA "$ca_crt" -CAkey "$ca_key" -CAcreateserial \
      -days 3650 -sha256 -out "$srv_crt" 2>/dev/null
    chown postgres:postgres "$srv_key" "$srv_crt" "$ca_crt"
    chmod 600 "$srv_key"
  fi
  set_conf ssl "on"
  set_conf ssl_cert_file "'$srv_crt'"
  set_conf ssl_key_file "'$srv_key'"
  # CA bundle the gateway mounts as its PostgreSQL trust store (prod sslmode=require)
  cp "$ca_crt" "$ca_bundle"
  chmod 644 "$ca_bundle"
  ok "PostgreSQL TLS enabled; CA bundle -> $ca_bundle"
else
  set_conf ssl "off"
  : > "$ca_bundle"   # placeholder so the gateway mount always resolves
  ok "PostgreSQL TLS disabled (plaintext)"
fi

# pg_hba: allow docker-bridge clients. host (plaintext, dev/test) + hostssl (TLS, prod).
hba_present() { grep -qF "$1" "$pghba"; }
if ! hba_present "host all all 0.0.0.0/0 md5"; then
  # keep local trust for admin tooling
  if ! hba_present "host all all 127.0.0.1/32 trust"; then
    sed -i "1i host all all 127.0.0.1/32 trust" "$pghba"
  fi
  {
    echo "host all all 0.0.0.0/0 md5"
    echo "hostssl all all 0.0.0.0/0 md5"
  } >> "$pghba"
  log "pg_hba updated to accept docker-bridge clients (md5)"
fi

# reload PG to apply config/hba
if command -v systemctl >/dev/null 2>&1 && systemctl cat postgresql >/dev/null 2>&1; then
  systemctl reload postgresql || pg_ctlcluster "$pgver" main reload
elif command -v service >/dev/null 2>&1; then
  service postgresql reload || pg_ctlcluster "$pgver" main reload
else
  pg_ctlcluster "$pgver" main reload
fi

# admin password (idempotent)
PGPASSWORD="$PG_ADMIN_PASS" psql -h 127.0.0.1 -p "$PG_PORT" -U "$PG_ADMIN_USER" -c "ALTER USER \"$PG_ADMIN_USER\" WITH PASSWORD '$PG_ADMIN_PASS';" 2>/dev/null \
  || PGPASSWORD="$PG_ADMIN_PASS" psql -h /var/run/postgresql -U "$PG_ADMIN_USER" -c "ALTER USER \"$PG_ADMIN_USER\" WITH PASSWORD '$PG_ADMIN_PASS';" 2>/dev/null \
  || log "could not set admin password via psql (continuing)"
ok "postgresql listening on 0.0.0.0:$PG_PORT"

# ---- 5. redis bind + port (must match docker/env GATEWAY_CLOUDROUTER_REDIS_PORT) -
redis_restart() {
  if command -v systemctl >/dev/null 2>&1 && systemctl cat redis-server >/dev/null 2>&1; then
    systemctl restart redis-server
  elif command -v service >/dev/null 2>&1; then
    service redis-server restart
  else
    redis-cli -p "$REDIS_PORT" shutdown nosave 2>/dev/null || true
    redis-server /etc/redis/redis.conf --daemonize yes 2>/dev/null || redis-server --daemonize yes
  fi
}
if [ -f /etc/redis/redis.conf ]; then
  redis_changed=false
  if ! grep -qE "^bind[[:space:]]+0.0.0.0" /etc/redis/redis.conf; then
    sed -i -E 's/^#?[[:space:]]*bind[[:space:]]+.*/bind 0.0.0.0/' /etc/redis/redis.conf
    redis_changed=true
  fi
  if grep -qE '^port[[:space:]]+[0-9]+' /etc/redis/redis.conf; then
    if ! grep -qE "^port[[:space:]]+$REDIS_PORT" /etc/redis/redis.conf; then
      sed -i -E '/^[[:space:]]*#/! s/^port[[:space:]]+[0-9]+/port '"$REDIS_PORT"'/' /etc/redis/redis.conf
      redis_changed=true
    fi
  else
    echo "port $REDIS_PORT" >> /etc/redis/redis.conf
    redis_changed=true
  fi
  if [ "$redis_changed" = true ]; then
    redis_restart
  fi
  ok "redis bound to 0.0.0.0:$REDIS_PORT"
fi

# ---- 6. provision per-environment databases --------------------------------
log "provisioning gateway databases (sdkwork-specs ENVIRONMENT_SPEC §7.1)"
export SDKWORK_DATABASE_ADMIN_HOST=127.0.0.1
export SDKWORK_DATABASE_ADMIN_PORT="$PG_PORT"
export SDKWORK_DATABASE_ADMIN_USERNAME="$PG_ADMIN_USER"
export SDKWORK_DATABASE_ADMIN_PASSWORD="$PG_ADMIN_PASS"
export PGPASSWORD="$PG_ADMIN_PASS"
bash "$repo_root/scripts/provision-external-databases.sh" development test staging production

echo
ok "HOST SERVICES READY"
echo "  PostgreSQL : 127.0.0.1:$PG_PORT (host.docker.internal in containers)"
echo "  Redis      : 127.0.0.1:$REDIS_PORT"
echo "  PG CA bundle: $ca_bundle"
echo "  Next: sudo bash scripts/deploy/apply.sh   (or: pnpm deploy:apply)"

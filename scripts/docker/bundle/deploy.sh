#!/usr/bin/env bash
# ============================================================================
# deploy.sh — generic deployer for the sdkwork-api-cloud-gateway unified
# install bundle
#
# One image, any environment, N instances per environment
# (DEPLOYMENT_SPEC.md §6 / PNPM_SCRIPT_SPEC.md §4.4).
#
# Layout (auto-detected):
#   bundle:  ./compose/docker-compose.bundle.yml + ./env/  + ./image.tar.gz
#   repo:    ../../docker-compose.bundle.yml  + ../../docker/env/
#
# Usage:
#   deploy.sh --environment <development|test|staging|demo|production> [options]
#     --replicas <N>     gateway instances to run (default 1; every env
#                        supports N; read from GATEWAY_REPLICAS when omitted)
#     --embedded         use the BUILT-IN PostgreSQL + Redis started in the
#                        per-environment deps project (opt-in; no managed
#                        databases required). Data survives --down in
#                        per-environment volumes; --down --purge removes it.
#     --external         use external PostgreSQL + Redis (the DEFAULT; also
#                        selectable with GATEWAY_DEPS_MODE=external)
#     --image-tag <tag>  override GATEWAY_IMAGE tag
#     --env-file <path>  absolute path of an external env file shared by all
#                        instances (see remote-deploy.sh; the Ubuntu config
#                        root /etc/sdkwork/api-gateway/docker/<env>.env is the
#                        convention). The file MUST already exist.
#     --set KEY VALUE    write one required value into the env file (refuses to
#                        overwrite an already-configured value)
#     --force-set K V    like --set but replaces even a configured value
#     --check-config     run the env preflight checks only (including the
#                        live external PostgreSQL/Redis connectivity probes
#                        in external mode); deploy nothing
#     --down             stop instances + the knowledgebase-rpc singleton
#     --purge            with --down: also delete volumes and network
#     --ps               show instance status
#     --logs [N]         gateway instance logs, instance N (default 1;
#                        prefer the explicit --instance form)
#     --instance <N>     select the instance for --logs (default 1)
#     --service <name>   compose service to read (default gateway)
#     --tail <N|all>     lines from the end (default 200)
#     --since <dur|time> start point, e.g. 15m or 2026-09-05T10:00:00Z
#     --follow           keep streaming (default: bounded read)
#     --stop             stop instances + the deps singleton (containers kept)
#     --start            start a previously --stop-ed environment again
#     --restart          restart the deps singleton + gateway instances
#                        (env/config changes need a full apply instead)
#     --dry-run          print the resolved commands only
#
# Instance port plan (deterministic, collision-free across environments):
#   instance i publishes GATEWAY_HOST_PORT + (i - 1) * 10 -> the container
#   listener GATEWAY_CONTAINER_PORT (default 3900; 8080 for webserver attach).
#   Example production (base 3913): instances publish 3913, 3923, 3933, ...
#   sdkwork-webserver load-balances across these host ports per environment.
#
# Examples:
#   deploy.sh --environment development
#   deploy.sh --environment production --replicas 3
#   deploy.sh --environment test --down --purge#
# Idempotent: re-running apply updates the existing stack in place.
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_PREFIX="[sdkwork-gateway-deploy]"

info() { printf '%s %s\n' "$LOG_PREFIX" "$*"; }
die()  { printf '%s ERROR: %s\n' "$LOG_PREFIX" "$*" >&2; exit 1; }
usage() { sed -n '2,/^set -euo pipefail$/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0; }

# --- layout autodetection ------------------------------------------------------
if [ -f "${SCRIPT_DIR}/compose/docker-compose.bundle.yml" ]; then
  # Bundle layout: deploy.sh sits in the bundle root.
  COMPOSE_DIR="${SCRIPT_DIR}/compose"
  ENV_DIR="${SCRIPT_DIR}/env"
  BUNDLE_IMAGE_TGZ="${SCRIPT_DIR}/image.tar.gz"
  BUNDLE_IMAGE_ENV="${SCRIPT_DIR}/image.env"
  POSTGRES_ASSETS_DIR="${SCRIPT_DIR}/postgres"
else
  # Repository layout: deploy.sh sits in scripts/docker/bundle/.
  COMPOSE_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  ENV_DIR="${COMPOSE_DIR}/docker/env"
  BUNDLE_IMAGE_TGZ=""
  BUNDLE_IMAGE_ENV=""
  POSTGRES_ASSETS_DIR="${COMPOSE_DIR}/docker/postgres"
fi
COMPOSE_FILE="${COMPOSE_DIR}/docker-compose.bundle.yml"
EMBEDDED_COMPOSE_FILE="${COMPOSE_DIR}/docker-compose.bundle.embedded.yml"

# --- defaults --------------------------------------------------------------------
ENVIRONMENT=""
REPLICAS=""
ACTION="apply"
PURGE="0"
IMAGE_TAG=""
OPT_ENV_FILE=""
SET_KEY=""
SET_VALUE=""
SET_FORCE="0"
DRY_RUN="0"
LOG_INSTANCE="1"
LOG_SERVICE="gateway"
LOG_TAIL="200"
LOG_SINCE=""
LOG_FOLLOW="0"
DEPS_MODE_FLAG=""

while [ $# -gt 0 ]; do
  case "$1" in
    --environment) ENVIRONMENT="$2"; shift 2 ;;
    --replicas)    REPLICAS="$2"; shift 2 ;;
    --embedded)    DEPS_MODE_FLAG="embedded"; shift ;;
    --external)    DEPS_MODE_FLAG="external"; shift ;;
    --image-tag)   IMAGE_TAG="$2"; shift 2 ;;
    --env-file)    OPT_ENV_FILE="$2"; shift 2 ;;
    --down)        ACTION="down"; shift ;;
    --purge)       PURGE="1"; shift ;;
    --ps)          ACTION="ps"; shift ;;
    --logs)        ACTION="logs"; LOG_INSTANCE="${2:-1}"; case "${2:-}" in ''|*[!0-9]*) shift ;; *) shift 2 ;; esac ;;
    --instance)    LOG_INSTANCE="$2"; shift 2 ;;
    --service)     LOG_SERVICE="$2"; shift 2 ;;
    --tail)        LOG_TAIL="$2"; shift 2 ;;
    --since)       LOG_SINCE="$2"; shift 2 ;;
    --follow)      LOG_FOLLOW="1"; shift ;;
    --stop)        ACTION="stop"; shift ;;
    --start)       ACTION="start"; shift ;;
    --restart)     ACTION="restart"; shift ;;
    --set)         [ $# -ge 3 ] || die "--set requires KEY and VALUE"; ACTION="set"; SET_KEY="$2"; SET_VALUE="$3"; shift 3 ;;
    --force-set)   [ $# -ge 3 ] || die "--force-set requires KEY and VALUE"; ACTION="set"; SET_KEY="$2"; SET_VALUE="$3"; SET_FORCE="1"; shift 3 ;;
    --check-config) ACTION="check-config"; shift ;;
    --dry-run)     DRY_RUN="1"; shift ;;
    -h|--help)     usage ;;
    *)             die "unsupported option: $1 (see --help)" ;;
  esac
done

[ -n "${ENVIRONMENT}" ] || die "--environment is required (development|test|staging|demo|production)"
case "${ENVIRONMENT}" in
  development|test|staging|demo|production) ;;
  *) die "unsupported environment: ${ENVIRONMENT} (development|test|staging|demo|production)" ;;
esac
command -v docker >/dev/null 2>&1 || { [ "${DRY_RUN}" = "1" ] || die "docker is required"; }
if [ "${DRY_RUN}" != "1" ]; then
  docker compose version >/dev/null 2>&1 || die "docker compose plugin is required (docker-compose-plugin)"
fi
[ -f "${COMPOSE_FILE}" ] || die "compose template missing: ${COMPOSE_FILE}"

# --- env file ---------------------------------------------------------------------
# An external --env-file (absolute path) is used verbatim and MUST exist; it
# lets every instance of an environment share one configuration file in the
# Ubuntu config root (APPLICATION_DEPLOY_LAYOUT_SPEC §3), e.g.
# /etc/sdkwork/api-gateway/docker/<environment>.env. Per-instance overrides
# are then looked up next to it (<environment>.i<index>.env).
if [ -n "${OPT_ENV_FILE}" ]; then
  ENV_FILE="${OPT_ENV_FILE}"
  [ -f "${ENV_FILE}" ] || die "external env file missing: ${ENV_FILE}"
  ENV_DIR="$(cd "$(dirname "${ENV_FILE}")" && pwd)"
  info "using external env file: ${ENV_FILE}"
else
  ENV_FILE="${ENV_DIR}/${ENVIRONMENT}.env"
  ENV_EXAMPLE="${ENV_DIR}/${ENVIRONMENT}.env.example"
  if [ ! -f "${ENV_FILE}" ]; then
    [ -f "${ENV_EXAMPLE}" ] || die "env file missing and no example: ${ENV_EXAMPLE}"
    cp "${ENV_EXAMPLE}" "${ENV_FILE}"
    info "created ${ENV_FILE} from example — fill secrets before exposing beyond localhost"
  fi
fi

# Read one KEY=value from the env file (no side effects; values are defaults).
env_key() {
  sed -n "s/^${1}=//p" "${ENV_FILE}" | tail -1 | tr -d '\r'
}

# --- env preflight (fail-closed, collects ALL gaps) -------------------------------
# The bundle env/<environment>.env is the single live configuration source
# (docker-install.md §2.1). Apply-like actions must never start from template
# placeholders or an incomplete attach contract. These checks encode field
# failures: 421 from a missing webserver attach alias, pool auth failures from
# <CHANGE_ME> passwords, and snowflake collisions from duplicated node ids.
#
# Design contract (PORTABILITY_SPEC.md / OPERATIONS_SPEC.md §3):
#   - READ-ONLY: a value that is already configured is kept as-is; deploy never
#     rewrites or re-prompts for configured secrets.
#   - COLLECTING: every missing required key is reported in ONE run with
#     foolproof fix instructions, instead of dying on the first miss.
#   - FAIL-CLOSED: deploy aborts while any required input is unconfigured.

is_unconfigured() {  # empty value or template placeholder
  case "$1" in
    ""|"<CHANGE_ME>"|"<change_me>"|"CHANGE_ME"|"changeme") return 0 ;;
    *) return 1 ;;
  esac
}

# Upsert one KEY=VALUE into the env file (BSD/GNU portable: awk + tmp + mv).
# Refuses to overwrite a configured value unless forced: re-deploys must run
# without touching live secrets. Returns 3 when it declined to overwrite.
set_env_key() {  # $1=key $2=value $3=force
  local key="$1" value="$2" force="${3:-0}" current secret="0"
  case "${key}" in
    ''|*[!A-Za-z0-9_]*) die "--set key must be a bare identifier (letters, digits, _): '${key}'" ;;
  esac
  current="$(env_key "${key}")"
  if ! is_unconfigured "${current}" && [ "${force}" != "1" ]; then
    info "${key} is already configured in ${ENV_FILE}; kept as-is (deploy never rewrites configured values)."
    info "to replace it deliberately, re-run with: --force-set ${key} '<new-value>'"
    return 3
  fi
  case "${key}" in
    *PASSWORD*|*SECRET*|*PEPPER*|*API_KEY*|*_KEY|*DATABASE_URL*|*_URL) secret="1" ;;
  esac
  awk -v k="${key}" -v v="${value}" '
    $0 ~ "^[[:space:]]*(export[[:space:]]+)?"k"=" { print k"="v; found=1; next }
    { print }
    END { if (!found) print k"="v }
  ' "${ENV_FILE}" > "${ENV_FILE}.tmp" && mv -f "${ENV_FILE}.tmp" "${ENV_FILE}"
  if [ "${secret}" = "1" ]; then
    info "set ${key}=<redacted> in ${ENV_FILE}"
  else
    info "set ${key}=${value} in ${ENV_FILE}"
  fi
  info "next step: re-run the deploy command, e.g. $0 --environment ${ENVIRONMENT}"
}

# Read-only required-key check. Reports the gap with concrete fix instructions
# and marks the run failed; never dies on the first miss.
require_env_key() {  # $1=key $2=label $3=example value
  local key="$1" label="$2" example="$3" value
  value="$(env_key "${key}")"
  if is_unconfigured "${value}"; then
    info "MISSING: ${key} (${label}) is not configured"
    info "  fix 1 (recommended): $0 --environment ${ENVIRONMENT} --set ${key} '<value>'"
    info "  fix 2: edit ${ENV_FILE} and set: ${key}=${example}"
    PREFLIGHT_FAIL="1"
  else
    info "OK: ${key} configured (existing value kept; deploy never rewrites it)"
  fi
}

PREFLIGHT_FAIL="0"

# --- dependency mode (resolved BEFORE preflight so external-only checks gate) --
# External PostgreSQL + Redis is the default posture; --embedded opts into the
# built-in databases started in the per-environment deps project. Priority:
# --embedded / --external flag > GATEWAY_DEPS_MODE in the env file > external.
DEPS_MODE="${DEPS_MODE_FLAG:-$(env_key GATEWAY_DEPS_MODE)}"
DEPS_MODE="${DEPS_MODE:-external}"
case "${DEPS_MODE}" in
  embedded|external) ;;
  *) die "unsupported GATEWAY_DEPS_MODE: ${DEPS_MODE} (external|embedded)" ;;
esac
COMPOSE_FILES=(-f "${COMPOSE_FILE}")
if [ "${DEPS_MODE}" = "embedded" ]; then
  [ -f "${EMBEDDED_COMPOSE_FILE}" ] || die "embedded compose override missing: ${EMBEDDED_COMPOSE_FILE}"
  [ -f "${POSTGRES_ASSETS_DIR}/entrypoint.sh" ] || die "embedded postgres assets missing: ${POSTGRES_ASSETS_DIR}/entrypoint.sh"
  COMPOSE_FILES+=(-f "${EMBEDDED_COMPOSE_FILE}")
  export POSTGRES_ASSETS_DIR
  info "dependency mode: embedded (built-in PostgreSQL + Redis in the deps project)"
else
  info "dependency mode: external (default; configure GATEWAY_POSTGRES_* / GATEWAY_CLOUDROUTER_REDIS_*)"
fi

# --- external dependency probes (foolproof, fail-closed) -------------------------
# From the deploy host, container-side endpoint names collapse to loopback:
# host.docker.internal is the docker-bridge alias for THIS host, so probing it
# directly would resolve nowhere outside a container. Remote hosts/IPs are
# probed verbatim.
probe_endpoint_host() {
  case "$1" in
    host.docker.internal|localhost) printf '127.0.0.1' ;;
    *) printf '%s' "$1" ;;
  esac
}

resolve_ca_source() {  # echo the host-side CA bundle path for the mount value
  local mount="$1" src=""
  [ -n "${mount}" ] || return 0
  case "${mount}" in
    /*) src="${mount}" ;;
    *)  src="${COMPOSE_DIR}/${mount}" ;;
  esac
  printf '%s' "${src}"
}

probe_external_postgres() {
  local host port user db pass url ssl ca_mount ca_src probe out rc=0
  host="$(env_key GATEWAY_POSTGRES_HOST)"
  port="$(env_key GATEWAY_POSTGRES_PORT)"; port="${port:-5432}"
  user="$(env_key GATEWAY_POSTGRES_USER)"
  db="$(env_key GATEWAY_POSTGRES_DB)"
  pass="$(env_key GATEWAY_POSTGRES_PASSWORD)"
  url="$(env_key SDKWORK_DATABASE_URL)"
  ssl="$(env_key GATEWAY_POSTGRES_SSL_MODE)"
  ca_mount="$(env_key GATEWAY_POSTGRES_CA_FILE_MOUNT)"
  ca_src="$(resolve_ca_source "${ca_mount}")"
  [ -f "${ca_src}" ] || ca_src=""
  if ! command -v psql >/dev/null 2>&1; then
    info "NOTE: psql not found on this host — skipped the live PostgreSQL probe (install postgresql-client for the full preflight)"
    return 0
  fi
  probe="$(probe_endpoint_host "${host}")"
  if [ -n "${url}" ]; then
    # SDKWORK_DATABASE_URL, when set, overrides compose's assembled URL
    # entirely (compose default folds the GATEWAY_POSTGRES_PASSWORD default
    # in) — so it IS the effective configuration; probe it verbatim
    # (endpoint name rewritten for host-side resolution).
    url="${url//host.docker.internal/127.0.0.1}"
    out="$(env PGCONNECT_TIMEOUT=5 psql "${url}" -w -Atc 'select 1' 2>&1)" || rc=$?
  else
    out="$(env PGCONNECT_TIMEOUT=5 \
      ${ssl:+PGSSLMODE="${ssl}"} \
      ${ca_src:+PGSSLROOTCERT="${ca_src}"} \
      PGPASSWORD="${pass}" \
      psql -h "${probe}" -p "${port}" -U "${user}" -d "${db}" -w -Atc 'select 1' 2>&1)" || rc=$?
  fi
  if [ "${rc}" = "0" ]; then
    info "OK: external PostgreSQL reachable and credentials accepted (${host}:${port}/${db} as ${user})"
    return 0
  fi
  case "${out}" in
    *"password authentication failed"*)
      die "external PostgreSQL rejected the configured credentials (${user}@${host}:${port}/${db}). On the database host, align the role password with the deploy env, e.g.: sudo -u postgres psql -c \"ALTER ROLE ${user} PASSWORD '<the GATEWAY_POSTGRES_PASSWORD value from ${ENV_FILE}>'\" — then re-run this command." ;;
    *"does not exist"*)
      die "external PostgreSQL database not found (${db} on ${host}:${port}). Create it on the database host, e.g.: sudo -u postgres psql -c \"CREATE DATABASE ${db} OWNER ${user};\" — then re-run this command." ;;
    *"Peer authentication failed"*)
      die "external PostgreSQL pg_hba.conf rejects password auth for ${user} from this host. Add a host-based scram-sha-256 line for ${user} in pg_hba.conf and reload PostgreSQL." ;;
    *"no pg_hba.conf entry"*)
      die "external PostgreSQL pg_hba.conf has no entry allowing ${user} to connect from this host. Add one (host ${db} ${user} <cidr> scram-sha-256) and reload PostgreSQL." ;;
    *"SSL"*"error"*|*"certificate verify failed"*)
      die "external PostgreSQL TLS handshake failed (${host}:${port}, GATEWAY_POSTGRES_SSL_MODE=$(env_key GATEWAY_POSTGRES_SSL_MODE)). Verify GATEWAY_POSTGRES_CA_FILE_MOUNT points at the real server trust chain ($(resolve_ca_source "${ca_mount}")) and that the server presents a cert issued under it." ;;
    *)
      die "external PostgreSQL unreachable (${host}:${port}): ${out}. Check the service is up (systemctl status postgresql), the host/port values in ${ENV_FILE}, and firewall rules." ;;
  esac
}

probe_external_redis() {
  local host port pass probe out rc=0
  host="$(env_key GATEWAY_CLOUDROUTER_REDIS_HOST)"
  port="$(env_key GATEWAY_CLOUDROUTER_REDIS_PORT)"; port="${port:-6379}"
  pass="$(env_key GATEWAY_CLOUDROUTER_REDIS_PASSWORD)"
  if ! command -v redis-cli >/dev/null 2>&1; then
    info "NOTE: redis-cli not found on this host — skipped the live Redis probe (install redis-tools for the full preflight)"
    return 0
  fi
  probe="$(probe_endpoint_host "${host}")"
  if [ -n "${pass}" ]; then
    out="$(REDISCLI_AUTH="${pass}" timeout 6 redis-cli -h "${probe}" -p "${port}" ping 2>&1)" || rc=$?
  else
    out="$(timeout 6 redis-cli -h "${probe}" -p "${port}" ping 2>&1)" || rc=$?
  fi
  if [ "${rc}" = "0" ] && [ "${out}" = "PONG" ]; then
    info "OK: external Redis reachable (${host}:${port})"
    return 0
  fi
  case "${out}" in
    *"WRONGPASS"*|*"NOAUTH"*|*"Client sent AUTH"*)
      die "external Redis rejected the configured auth (${host}:${port}). Align GATEWAY_CLOUDROUTER_REDIS_PASSWORD in ${ENV_FILE} with the Redis requirepass value (empty value = passwordless Redis), then re-run this command." ;;
    *)
      die "external Redis unreachable (${host}:${port}): ${out}. Check the service is up (systemctl status redis-server), the host/port values in ${ENV_FILE}, and firewall rules." ;;
  esac
}

case "${ACTION}" in
  apply|start|restart|check-config)
    pre_host_port="$(env_key GATEWAY_HOST_PORT)"
    if is_unconfigured "${pre_host_port}"; then
      info "MISSING: GATEWAY_HOST_PORT (host management port) is not configured"
      info "  fix 1 (recommended): $0 --environment ${ENVIRONMENT} --set GATEWAY_HOST_PORT <port>   e.g. 3910"
      info "  fix 2: edit ${ENV_FILE} and set: GATEWAY_HOST_PORT=3910"
      PREFLIGHT_FAIL="1"
    else
      case "${pre_host_port}" in ''|*[!0-9]*) die "env preflight: GATEWAY_HOST_PORT must be numeric (got '${pre_host_port}')" ;; esac
      info "OK: GATEWAY_HOST_PORT=${pre_host_port}"
    fi
    pre_container_port="$(env_key GATEWAY_CONTAINER_PORT)"
    if is_unconfigured "${pre_container_port}"; then
      info "MISSING: GATEWAY_CONTAINER_PORT (in-container listen port) is not configured"
      info "  fix 1 (recommended): $0 --environment ${ENVIRONMENT} --set GATEWAY_CONTAINER_PORT <port>   e.g. 8080"
      info "  fix 2: edit ${ENV_FILE} and set: GATEWAY_CONTAINER_PORT=8080"
      PREFLIGHT_FAIL="1"
    else
      case "${pre_container_port}" in ''|*[!0-9]*) die "env preflight: GATEWAY_CONTAINER_PORT must be numeric (got '${pre_container_port}')" ;; esac
      info "OK: GATEWAY_CONTAINER_PORT=${pre_container_port}"
    fi
    pre_node_id="$(env_key GATEWAY_IM_ID_NODE_ID)"
    if is_unconfigured "${pre_node_id}"; then
      info "MISSING: GATEWAY_IM_ID_NODE_ID (snowflake node id, unique per host) is not configured"
      info "  fix 1 (recommended): $0 --environment ${ENVIRONMENT} --set GATEWAY_IM_ID_NODE_ID <id>   e.g. 21-25 per environment"
      info "  fix 2: edit ${ENV_FILE} and set: GATEWAY_IM_ID_NODE_ID=21"
      PREFLIGHT_FAIL="1"
    else
      case "${pre_node_id}" in ''|*[!0-9]*) die "env preflight: GATEWAY_IM_ID_NODE_ID must be numeric (got '${pre_node_id}')" ;; esac
      info "OK: GATEWAY_IM_ID_NODE_ID=${pre_node_id}"
    fi
    # Database credentials: GATEWAY_POSTGRES_PASSWORD, or the full
    # SDKWORK_DATABASE_URL when credentials contain URL-reserved characters.
    pre_db_password="$(env_key GATEWAY_POSTGRES_PASSWORD)"
    pre_db_url="$(env_key SDKWORK_DATABASE_URL)"
    if is_unconfigured "${pre_db_password}" && is_unconfigured "${pre_db_url}"; then
      info "MISSING: database credentials — configure GATEWAY_POSTGRES_PASSWORD (or SDKWORK_DATABASE_URL)"
      info "  fix 1 (recommended): $0 --environment ${ENVIRONMENT} --set GATEWAY_POSTGRES_PASSWORD '<your-db-password>'"
      info "  fix 2: edit ${ENV_FILE} and set: GATEWAY_POSTGRES_PASSWORD=<your-db-password>"
      info "  fix 3 (URL form, when the password contains URL-reserved characters):"
      info "         $0 --environment ${ENVIRONMENT} --set SDKWORK_DATABASE_URL 'postgresql://<user>:<password>@<host>:5432/<db>?sslmode=disable'"
      PREFLIGHT_FAIL="1"
    else
      info "OK: database credentials configured (existing value kept; deploy never rewrites it)"
    fi
    [ "${pre_db_password}" = "<CHANGE_ME>" ] && die "env preflight: GATEWAY_POSTGRES_PASSWORD is still the <CHANGE_ME> template placeholder (docker-install.md §4.1); fix with: $0 --environment ${ENVIRONMENT} --set GATEWAY_POSTGRES_PASSWORD '<real-password>'"
    require_env_key GATEWAY_POSTGRES_HOST "PostgreSQL host the gateway connects to" "host.docker.internal"
    pre_pg_host="$(env_key GATEWAY_POSTGRES_HOST)"
    case "${pre_pg_host}" in
      *example.com*|*example.org*|*your-db-host*)
        info "MISSING: GATEWAY_POSTGRES_HOST is still the documentation placeholder '${pre_pg_host}'"
        info "  fix 1 (recommended): $0 --environment ${ENVIRONMENT} --set GATEWAY_POSTGRES_HOST '<real-host>'   e.g. host.docker.internal for the co-located host instance"
        info "  fix 2: edit ${ENV_FILE} and set: GATEWAY_POSTGRES_HOST=host.docker.internal"
        PREFLIGHT_FAIL="1" ;;
    esac
    require_env_key GATEWAY_POSTGRES_PORT "PostgreSQL port" "5432"
    pre_pg_port="$(env_key GATEWAY_POSTGRES_PORT)"
    case "${pre_pg_port}" in ''|*[!0-9]*) die "env preflight: GATEWAY_POSTGRES_PORT must be numeric (got '${pre_pg_port}')" ;; esac
    require_env_key GATEWAY_POSTGRES_USER "PostgreSQL user" "sdkwork_ai_<env>"
    require_env_key GATEWAY_POSTGRES_DB "PostgreSQL database name" "sdkwork_ai_<env>"
    pre_allowed_hosts="$(env_key GATEWAY_ALLOWED_HOSTS)"
    if is_unconfigured "${pre_allowed_hosts}"; then
      info "MISSING: GATEWAY_ALLOWED_HOSTS (host allowlist, must include attach aliases) is not configured"
      info "  fix: edit ${ENV_FILE} and set GATEWAY_ALLOWED_HOSTS=<comma-separated hosts>,gateway,sdkwork-api-cloud-gateway"
      PREFLIGHT_FAIL="1"
    else
      case ",${pre_allowed_hosts}," in
        *,gateway,*) ;;
        *) die "env preflight: GATEWAY_ALLOWED_HOSTS must contain the webserver attach alias 'gateway' (sidecars get 421 otherwise)" ;;
      esac
      case ",${pre_allowed_hosts}," in
        *,sdkwork-api-cloud-gateway,*) ;;
        *) die "env preflight: GATEWAY_ALLOWED_HOSTS must contain the compose network alias 'sdkwork-api-cloud-gateway'" ;;
      esac
      info "OK: GATEWAY_ALLOWED_HOSTS includes both attach aliases"
    fi
    # TLS posture: when the env enables GATEWAY_POSTGRES_CA_FILE_MOUNT the
    # source must already be a regular file. Docker silently turns a missing
    # bind-mount source into an empty directory (both on the host and inside
    # the container); rustls then fails with "Is a directory (os error 21)"
    # and the gateway crash-loops with no error line (docker-install.md §4.2).
    pre_ca_mount="$(env_key GATEWAY_POSTGRES_CA_FILE_MOUNT)"
    if [ -n "${pre_ca_mount}" ]; then
      case "${pre_ca_mount}" in
        /*) pre_ca_src="${pre_ca_mount}" ;;
        *) pre_ca_src="${COMPOSE_DIR}/${pre_ca_mount}" ;;
      esac
      [ -f "${pre_ca_src}" ] || die "env preflight: GATEWAY_POSTGRES_CA_FILE_MOUNT source is not a regular file: ${pre_ca_src} (copy the host PostgreSQL CA bundle there, e.g. cp /etc/sdkwork/postgres-tls/ca.crt <path>)"
    fi
    # --- external-mode-only configuration (embedded mode has no external DB) ---
    if [ "${DEPS_MODE}" = "external" ]; then
      # Redis runs externally as well: without an endpoint the gateway
      # sidecar has nowhere to reach the shared cache/session store.
      require_env_key GATEWAY_CLOUDROUTER_REDIS_HOST "external Redis host" "host.docker.internal"
      require_env_key GATEWAY_CLOUDROUTER_REDIS_PORT "external Redis port" "6379"
      pre_redis_port="$(env_key GATEWAY_CLOUDROUTER_REDIS_PORT)"
      case "${pre_redis_port}" in ''|*[!0-9]*) die "env preflight: GATEWAY_CLOUDROUTER_REDIS_PORT must be numeric (got '${pre_redis_port}')" ;; esac
      # SSL mode must be a real libpq/rustls value, and any verifying mode
      # needs the CA mount — the IM pool verifies the server chain even in
      # `require` mode and crash-loops without it (docker-install.md §4.2).
      pre_ssl_mode="$(env_key GATEWAY_POSTGRES_SSL_MODE)"
      pre_ssl_mode="${pre_ssl_mode:-disable}"
      case "${pre_ssl_mode}" in
        disable|allow|prefer|require|verify-ca|verify-full)
          info "OK: GATEWAY_POSTGRES_SSL_MODE=${pre_ssl_mode}" ;;
        *)
          die "env preflight: GATEWAY_POSTGRES_SSL_MODE='${pre_ssl_mode}' is invalid (disable|allow|prefer|require|verify-ca|verify-full); fix with: $0 --environment ${ENVIRONMENT} --set GATEWAY_POSTGRES_SSL_MODE require" ;;
      esac
      case "${pre_ssl_mode}" in
        require|verify-ca|verify-full)
          if is_unconfigured "${pre_ca_mount}"; then
            info "MISSING: GATEWAY_POSTGRES_CA_FILE_MOUNT is required while GATEWAY_POSTGRES_SSL_MODE=${pre_ssl_mode} (the IM pool verifies the server certificate even in require mode)"
            info "  fix 1 (recommended): $0 --environment ${ENVIRONMENT} --set GATEWAY_POSTGRES_CA_FILE_MOUNT './docker/ca/postgres-ca-bundle.crt'"
            info "  fix 2: edit ${ENV_FILE} and set: GATEWAY_POSTGRES_CA_FILE_MOUNT=./docker/ca/postgres-ca-bundle.crt"
            info "  then place the host PostgreSQL trust chain there, e.g.:"
            info "    mkdir -p <bundle>/compose/docker/ca && cat /etc/sdkwork/postgres-tls/ca.crt /etc/sdkwork/postgres-tls/server.crt > <bundle>/compose/docker/ca/postgres-ca-bundle.crt"
            PREFLIGHT_FAIL="1"
          fi ;;
      esac
    else
      info "dependency mode is embedded — external database host/SSL checks skipped"
    fi
    # Node ids must be unique across every env file sharing this ENV_DIR so
    # co-located fleets never collide (DEPLOYMENT_SPEC §6).
    if [ -n "${pre_node_id}" ]; then
      for pre_other in "${ENV_DIR}"/*.env; do
        [ -f "${pre_other}" ] || continue
        [ "${pre_other}" = "${ENV_FILE}" ] && continue
        case "${pre_other}" in *.env.example) continue ;; esac
        pre_other_node="$(sed -n 's/^GATEWAY_IM_ID_NODE_ID=//p' "${pre_other}" | tail -1 | tr -d '\r')"
        if [ -n "${pre_other_node}" ] && [ "${pre_other_node}" = "${pre_node_id}" ]; then
          die "env preflight: GATEWAY_IM_ID_NODE_ID=${pre_node_id} is also used by ${pre_other}; co-located fleets require unique node ids (DEPLOYMENT_SPEC §6)"
        fi
      done
    fi
    if [ "${ENVIRONMENT}" = "production" ] || [ "${ENVIRONMENT}" = "staging" ]; then
      case "$(env_key GATEWAY_CLOUDROUTER_API_KEY_PEPPER)" in
        *development-only*)
          info "WARNING: ${ENVIRONMENT} still uses development-only Cloud Router secrets; rotate before real traffic (docker-install.md §4.1)"
          info "  fix: $0 --environment ${ENVIRONMENT} --force-set GATEWAY_CLOUDROUTER_API_KEY_PEPPER '<strong-random-value>'"
          ;;
      esac
    fi
    if [ "${PREFLIGHT_FAIL}" = "1" ]; then
      info "env preflight FAILED: configure every MISSING key above (pick one fix per key), then re-run this command. Nothing was deployed."
      exit 1
    fi
    # Live connectivity probes: run only once every required key is present,
    # so a probe failure is a real environment problem, not a missing key.
    # Probing needs no docker and never mutates anything on the databases.
    if [ "${DEPS_MODE}" = "external" ] && [ "${DRY_RUN}" != "1" ]; then
      probe_external_postgres
      probe_external_redis
    fi
    info "env preflight passed: all required inputs are configured (existing values kept; deploy never rewrites them)"
    ;;
esac

# --- image resolution ----------------------------------------------------------------
# Priority: --image-tag > bundle image.env > env-file GATEWAY_IMAGE tag > 0.1.0.
if [ -z "${IMAGE_TAG}" ] && [ -n "${BUNDLE_IMAGE_ENV}" ] && [ -f "${BUNDLE_IMAGE_ENV}" ]; then
  IMAGE_TAG="$(sed -n 's/^SDKWORK_API_CLOUD_GATEWAY_IMAGE_TAG=//p' "${BUNDLE_IMAGE_ENV}" | tail -1 | tr -d '\r')"
fi
if [ -z "${IMAGE_TAG}" ]; then
  IMAGE_TAG="$(env_key GATEWAY_IMAGE | sed 's/.*://')"
fi
IMAGE_TAG="${IMAGE_TAG:-0.1.0}"
# Image base: bundle deployments always use the canonical registry name (the
# loaded image.tar.gz restores it verbatim); repo-layout deployments honor the
# env-file GATEWAY_IMAGE repository so a locally built image is used as-is.
IMAGE_BASE="registry.sdkwork.com/apps/sdkwork-api-cloud-gateway"
if [ -z "${BUNDLE_IMAGE_ENV}" ]; then
  env_image="$(env_key GATEWAY_IMAGE)"
  case "${env_image}" in
    *:*) env_image_base="${env_image%:*}" ;;
    *)   env_image_base="${env_image}" ;;
  esac
  if [ -n "${env_image_base}" ]; then
    IMAGE_BASE="${env_image_base}"
  fi
fi
IMAGE_REF="${IMAGE_BASE}:${IMAGE_TAG}"
export GATEWAY_IMAGE="${IMAGE_REF}"
export GATEWAY_ENVIRONMENT="${ENVIRONMENT}"

# --- replica count ---------------------------------------------------------------------
if [ -z "${REPLICAS}" ]; then
  REPLICAS="$(env_key GATEWAY_REPLICAS)"
fi
REPLICAS="${REPLICAS:-1}"
case "${REPLICAS}" in ''|*[!0-9]*) die "--replicas must be a positive integer" ;; esac
[ "${REPLICAS}" -ge 1 ] || die "--replicas must be a positive integer"

HOST_PORT_BASE="$(env_key GATEWAY_HOST_PORT)"
HOST_PORT_BASE="${HOST_PORT_BASE:-3910}"
case "${HOST_PORT_BASE}" in ''|*[!0-9]*) die "GATEWAY_HOST_PORT must be a positive integer" ;; esac

DEPS_PROJECT="sdkwork-api-cloud-gateway-${ENVIRONMENT}-deps"
NETWORK="sdkwork-api-cloud-gateway-${ENVIRONMENT}"
HEALTH_TIMEOUT="${SDKWORK_DEPLOY_HEALTH_TIMEOUT:-900}"

run() {
  info "$*"
  if [ "${DRY_RUN}" != "1" ]; then "$@"; fi
}

# --- shared resources (per environment, shared by all instances) ------------------
ensure_shared_resources() {
  run docker network create "${NETWORK}" 2>/dev/null || info "network ${NETWORK} already present"
  for suffix in secrets data; do
    run docker volume create "${NETWORK}-${suffix}" 2>/dev/null \
      || info "volume ${NETWORK}-${suffix} already present"
  done
  if [ "${DEPS_MODE}" = "embedded" ]; then
    for suffix in postgres-data redis-data postgres-tls; do
      run docker volume create "${NETWORK}-${suffix}" 2>/dev/null \
        || info "volume ${NETWORK}-${suffix} already present"
    done
  fi
}

# --- image -------------------------------------------------------------------------
ensure_image() {
  if [ "${DRY_RUN}" = "1" ]; then
    info "dry-run: would ensure image ${IMAGE_REF} (load bundle image.tar.gz when missing)"
    return 0
  fi
  if docker image inspect "${IMAGE_REF}" >/dev/null 2>&1; then
    info "image present: ${IMAGE_REF}"
    return 0
  fi
  if [ -n "${BUNDLE_IMAGE_TGZ}" ] && [ -f "${BUNDLE_IMAGE_TGZ}" ]; then
    run docker load -i "${BUNDLE_IMAGE_TGZ}"
    return 0
  fi
  die "image ${IMAGE_REF} not found and no bundle image.tar.gz beside this script; build with: pnpm build:container:install"
}

# --- knowledgebase-rpc singleton (per environment) -----------------------------------
# Compose interpolates the whole file (including gateway.ports) even for
# deps-only projects, so per-instance inputs must be exported here too.
export_instance_env() {
  local index="$1"
  export GATEWAY_INSTANCE_HOST_PORT=$((HOST_PORT_BASE + (index - 1) * 10))
  export GATEWAY_PROFILE_ID="standalone.${ENVIRONMENT}.i${index}"
  # Unique node identity per instance (DEPLOYMENT_SPEC §6): instance 1 keeps
  # the env-file base node id; instances 2..N advance it by one so snowflake
  # id generation never collides between replicas of the same environment.
  local node_base
  node_base="$(env_key GATEWAY_IM_ID_NODE_ID)"
  node_base="${node_base:-2}"
  export GATEWAY_IM_ID_NODE_ID=$((node_base + index - 1))
  if [ "${index}" -eq 1 ]; then
    # Instance 1 owns migration; the env-file value (default true) applies.
    export GATEWAY_MIGRATE_ON_START="$(env_key GATEWAY_MIGRATE_ON_START)"
    export GATEWAY_MIGRATE_ON_START="${GATEWAY_MIGRATE_ON_START:-true}"
  else
    export GATEWAY_MIGRATE_ON_START="false"
  fi
}

instance_env_args() {
  local index="$1"
  local override="${ENV_DIR}/${ENVIRONMENT}.i${index}.env"
  if [ -f "${override}" ]; then
    info "instance ${index} config override: ${override}" >&2
    printf '%s' "--env-file ${ENV_FILE} --env-file ${override}"
  else
    printf '%s' "--env-file ${ENV_FILE}"
  fi
}

start_deps() {
  export_instance_env 1
  run docker compose -p "${DEPS_PROJECT}" --env-file "${ENV_FILE}" \
    "${COMPOSE_FILES[@]}" --profile deps up -d
  wait_container_healthy "${DEPS_PROJECT}" knowledgebase-rpc "${HEALTH_TIMEOUT}"
}

start_instance() {
  local index="$1"
  local project="${NETWORK}-i${index}"
  # Shell env wins over the --env-file in compose; these are per-instance inputs.
  export_instance_env "${index}"
  run docker compose -p "${project}" $(instance_env_args "${index}") \
    "${COMPOSE_FILES[@]}" --profile instance up -d
  wait_container_healthy "${project}" gateway "${HEALTH_TIMEOUT}"
}

wait_container_healthy() {
  local project="$1" service="$2" timeout="$3"
  [ "${DRY_RUN}" = "1" ] && return 0
  local waited=0 cid status
  while true; do
    cid="$(docker compose -p "${project}" ps -q "${service}" 2>/dev/null || true)"
    if [ -n "${cid}" ]; then
      status="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "${cid}" 2>/dev/null || true)"
      case "${status}" in
        healthy|running)
          info "${project}/${service} is ${status} (waited ${waited}s)"
          return 0
          ;;
      esac
    fi
    if [ "${waited}" -ge "${timeout}" ]; then
      die "${project}/${service} not healthy after ${timeout}s"
    fi
    sleep 5
    waited=$((waited + 5))
  done
}

apply() {
  ensure_shared_resources
  ensure_image
  # The knowledgebase-rpc singleton starts first (it provisions the shared
  # secrets material); then instance 1 migrates and reaches health before
  # instances 2..N start (DEPLOYMENT_SPEC.md §6).
  start_deps
  local index
  for index in $(seq 1 "${REPLICAS}"); do
    start_instance "${index}"
  done
  info "environment ${ENVIRONMENT}: ${REPLICAS} gateway instance(s) applied"
  info "instance host ports: $((HOST_PORT_BASE)) (+10 per instance) -> container ${GATEWAY_CONTAINER_PORT:-3900}"
  info "health: curl http://127.0.0.1:${HOST_PORT_BASE}/healthz"
}

# --- instance project discovery -------------------------------------------------------
# Compose project names follow <environment>-i<index>; discovery reads the
# com.docker.compose.project label so teardown never depends on the replica
# count recorded in the env file (an apply with --replicas N does not write it
# back). This keeps `--down --purge` instance-count agnostic.
discover_instance_projects() {
  [ "${DRY_RUN}" = "1" ] && return 0
  docker ps -a --filter "label=com.docker.compose.project" \
    --format '{{.Label "com.docker.compose.project"}}' 2>/dev/null | sort -u \
    | grep -E "^${NETWORK}-i[0-9]+$" || true
}

down() {
  local index
  for index in $(seq 1 "${REPLICAS}"); do
    export_instance_env "${index}"
    run docker compose -p "${NETWORK}-i${index}" $(instance_env_args "${index}") \
      "${COMPOSE_FILES[@]}" --profile instance down
  done
  # Safety net: tear down instance projects beyond --replicas (e.g. left by an
  # earlier apply with a larger --replicas value or GATEWAY_REPLICAS).
  local extra extra_index
  for extra in $(discover_instance_projects); do
    extra_index="${extra##*-i}"
    case "${extra_index}" in
      ''|*[!0-9]*) continue ;;
    esac
    if [ "${extra_index}" -le "${REPLICAS}" ]; then continue; fi
    info "discovered extra instance project beyond replicas ${REPLICAS}: ${extra}"
    run docker compose -p "${extra}" --env-file "${ENV_FILE}" \
      "${COMPOSE_FILES[@]}" --profile instance down
  done
  export_instance_env 1
  run docker compose -p "${DEPS_PROJECT}" --env-file "${ENV_FILE}" \
    "${COMPOSE_FILES[@]}" --profile deps down
  if [ "${PURGE}" = "1" ]; then
    info "purging per-environment network and volumes (${ENVIRONMENT})"
    run docker network rm "${NETWORK}" 2>/dev/null || true
    for suffix in secrets data; do
      run docker volume rm "${NETWORK}-${suffix}" 2>/dev/null || true
    done
    if [ "${DEPS_MODE}" = "embedded" ]; then
      for suffix in postgres-data redis-data postgres-tls; do
        run docker volume rm "${NETWORK}-${suffix}" 2>/dev/null || true
      done
    fi
  fi
  info "environment ${ENVIRONMENT} down"
}

ps() {
  local index
  for index in $(seq 1 "${REPLICAS}"); do
    export_instance_env "${index}"
    info "instance ${index}:"
    docker compose -p "${NETWORK}-i${index}" $(instance_env_args "${index}") \
      "${COMPOSE_FILES[@]}" --profile instance ps
  done
  export_instance_env 1
  info "knowledgebase-rpc singleton:"
  docker compose -p "${DEPS_PROJECT}" --env-file "${ENV_FILE}" \
    "${COMPOSE_FILES[@]}" --profile deps ps
}

# --- stop / start / restart ----------------------------------------------------
# `--stop` keeps the containers (fast `--start` later); `--down` removes them.
# `--restart` bounces the processes only — after env/config edits run a full
# apply so compose recreates the containers with the new values.
stop_fleet() {
  local index extra extra_index
  for index in $(seq 1 "${REPLICAS}"); do
    export_instance_env "${index}"
    run docker compose -p "${NETWORK}-i${index}" $(instance_env_args "${index}") \
      "${COMPOSE_FILES[@]}" --profile instance stop
  done
  for extra in $(discover_instance_projects); do
    extra_index="${extra##*-i}"
    case "${extra_index}" in
      ''|*[!0-9]*) continue ;;
    esac
    if [ "${extra_index}" -le "${REPLICAS}" ]; then continue; fi
    info "discovered extra instance project beyond replicas ${REPLICAS}: ${extra}"
    run docker compose -p "${extra}" --env-file "${ENV_FILE}" \
      "${COMPOSE_FILES[@]}" --profile instance stop
  done
  export_instance_env 1
  run docker compose -p "${DEPS_PROJECT}" --env-file "${ENV_FILE}" \
    "${COMPOSE_FILES[@]}" --profile deps stop
  info "environment ${ENVIRONMENT} stopped (containers kept; start again with --start)"
}

start_fleet() {
  export_instance_env 1
  run docker compose -p "${DEPS_PROJECT}" --env-file "${ENV_FILE}" \
    "${COMPOSE_FILES[@]}" --profile deps start
  wait_container_healthy "${DEPS_PROJECT}" knowledgebase-rpc "${HEALTH_TIMEOUT}"
  local index
  for index in $(seq 1 "${REPLICAS}"); do
    export_instance_env "${index}"
    run docker compose -p "${NETWORK}-i${index}" $(instance_env_args "${index}") \
      "${COMPOSE_FILES[@]}" --profile instance start
    wait_container_healthy "${NETWORK}-i${index}" gateway "${HEALTH_TIMEOUT}"
  done
  info "environment ${ENVIRONMENT} started"
}

restart_fleet() {
  export_instance_env 1
  info "restarting the deps singleton (knowledgebase-rpc + built-in databases if any)"
  run docker compose -p "${DEPS_PROJECT}" --env-file "${ENV_FILE}" \
    "${COMPOSE_FILES[@]}" --profile deps restart
  wait_container_healthy "${DEPS_PROJECT}" knowledgebase-rpc "${HEALTH_TIMEOUT}"
  local index
  for index in $(seq 1 "${REPLICAS}"); do
    export_instance_env "${index}"
    run docker compose -p "${NETWORK}-i${index}" $(instance_env_args "${index}") \
      "${COMPOSE_FILES[@]}" --profile instance restart gateway
    wait_container_healthy "${NETWORK}-i${index}" gateway "${HEALTH_TIMEOUT}"
  done
  info "environment ${ENVIRONMENT} restarted"
}

logs() {
  local index="${LOG_INSTANCE}"
  case "${index}" in ''|*[!0-9]*) die "--logs/--instance expects an instance number" ;; esac
  case "${LOG_TAIL}" in all) ;; ''|*[!0-9]*) die "--tail expects a positive integer or 'all'" ;; esac
  export_instance_env "${index}"
  local args=(docker compose -p "${NETWORK}-i${index}" $(instance_env_args "${index}") \
    "${COMPOSE_FILES[@]}" --profile instance logs --tail "${LOG_TAIL}")
  if [ -n "${LOG_SINCE}" ]; then args+=(--since "${LOG_SINCE}"); fi
  if [ "${LOG_FOLLOW}" = "1" ]; then args+=(--follow); fi
  if [ -n "${LOG_SERVICE}" ]; then args+=("${LOG_SERVICE}"); fi
  "${args[@]}"
}

case "${ACTION}" in
  apply)   apply ;;
  down)    down ;;
  ps)      ps ;;
  logs)    logs ;;
  stop)    stop_fleet ;;
  start)   start_fleet ;;
  restart) restart_fleet ;;
  check-config) info "configuration check only: nothing was deployed" ;;
  set)
    [ -n "${SET_KEY}" ] || die "--set requires KEY and VALUE"
    set_status="0"
    set_env_key "${SET_KEY}" "${SET_VALUE}" "${SET_FORCE}" || set_status=$?
    exit "${set_status}"
    ;;
  *) die "unsupported action: ${ACTION}" ;;
esac

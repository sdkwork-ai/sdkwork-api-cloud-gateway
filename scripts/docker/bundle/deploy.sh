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
#     --down             stop instances + the knowledgebase-rpc singleton
#     --purge            with --down: also delete volumes and network
#     --ps               show instance status
#     --logs [N]         follow gateway instance N logs (default 1)
#     --stop             stop instances + the deps singleton (containers kept)
#     --start            start a previously --stop-ed environment again
#     --restart          restart the deps singleton + gateway instances
#                        (env/config changes need a full apply instead)
#     --dry-run          print the resolved commands only
#
# Instance port plan (deterministic, collision-free across environments):
#   instance i publishes GATEWAY_HOST_PORT + (i - 1) * 10 -> container 3900.
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
DRY_RUN="0"
LOG_INSTANCE="1"
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
    --stop)        ACTION="stop"; shift ;;
    --start)       ACTION="start"; shift ;;
    --restart)     ACTION="restart"; shift ;;
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

# --- dependency mode -----------------------------------------------------------------
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
  info "instance host ports: $((HOST_PORT_BASE)) (+10 per instance) -> container 3900"
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
  case "${index}" in ''|*[!0-9]*) die "--logs expects an instance number" ;; esac
  export_instance_env "${index}"
  docker compose -p "${NETWORK}-i${index}" $(instance_env_args "${index}") \
    "${COMPOSE_FILES[@]}" --profile instance logs -f --tail 200 gateway
}

case "${ACTION}" in
  apply)   apply ;;
  down)    down ;;
  ps)      ps ;;
  logs)    logs ;;
  stop)    stop_fleet ;;
  start)   start_fleet ;;
  restart) restart_fleet ;;
  *) die "unsupported action: ${ACTION}" ;;
esac

#!/usr/bin/env bash
# ============================================================================
# release.sh — release lifecycle for the sdkwork-api-cloud-gateway Docker
# deployment (DEPLOYMENT_SPEC.md §6 / RELEASE_SPEC.md)
#
# Wraps deploy.sh (apply/stop/start/...) with a versioned release lifecycle:
#
#   deploy    upgrade the environment to an image version, gated by an HTTP
#             health probe; automatic rollback to the previous version when
#             the gate fails
#   rollback  go back to the previous successful version (or --to <version>)
#   status    which version is deployed, which image the containers actually
#             run, and a live health probe of every instance
#   history   append-only release ledger (release-state/<env>/ledger.jsonl)
#   verify    health-probe every instance only (no changes)
#
# State layout (SDKWORK_RELEASE_STATE_DIR overrides the root):
#   <state-root>/<environment>/current.json    deployed version snapshot
#   <state-root>/<environment>/ledger.jsonl    append-only release history
#
# Default state root: bundle layout -> ./release-state beside deploy.sh;
# repo layout -> <repo-root>/release-state; --env-file layout ->
# release-state beside the shared env file (e.g. /etc/sdkwork/api-gateway/docker).
#
# Usage:
#   release.sh <deploy|rollback|status|history|verify|versions> --environment <env> [options]
#
# Common options (forwarded to deploy.sh where applicable):
#   --image-tag <tag>   deploy target version (deploy only; default: bundle
#                       image.env / env-file GATEWAY_IMAGE)
#   --to <version>      explicit rollback target (rollback only; default:
#                       previous successful version from the ledger)
#   --replicas <N>      gateway instances (default: env-file GATEWAY_REPLICAS / 1)
#   --embedded          built-in PostgreSQL + Redis (default external)
#   --external          external PostgreSQL + Redis (default)
#   --env-file <path>   shared external env file (same convention as deploy.sh)
#   --health-timeout S  probe budget per attempt set (default 180s)
#   --no-auto-rollback  keep the failed deployment in place on probe failure
#   --skip-verify       skip the bundle image.tar.gz sha256 integrity check
#                       (deploy only, when the tarball is about to be loaded)
#   --lock-timeout S    wait budget for the per-environment release lock
#                       (default 120s; mutating actions only)
#   --dry-run           print the resolved plan without changing anything
#   -h, --help          show this help
#
# Examples:
#   release.sh deploy   --environment staging                 # upgrade in place
#   release.sh deploy   --environment staging --image-tag 0.1.2
#   release.sh rollback --environment staging                 # one step back
#   release.sh rollback --environment staging --to 0.1.0
#   release.sh status   --environment staging
#   release.sh history  --environment staging
#   release.sh versions --environment staging                 # local image tags
#
# Semantics notes (aligned with Helm/Argo Rollouts vocabulary):
#   - Mutating actions (deploy/rollback) run under a per-environment lock;
#     concurrent releases fail fast instead of interleaving.
#   - Database migrations are FORWARD-ONLY: rollback swaps the image but never
#     reverts applied migrations. Releases must ship backward-compatible
#     migrations (expand/contract) so the previous version stays runnable.
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_PREFIX="[sdkwork-gateway-release]"

info() { printf '%s %s\n' "$LOG_PREFIX" "$*"; }
warn() { printf '%s WARNING: %s\n' "$LOG_PREFIX" "$*" >&2; }
die()  { printf '%s ERROR: %s\n' "$LOG_PREFIX" "$*" >&2; exit 1; }
usage() { sed -n '2,/^set -euo pipefail$/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0; }

# --- layout autodetection (mirrors deploy.sh) ---------------------------------
if [ -f "${SCRIPT_DIR}/compose/docker-compose.bundle.yml" ]; then
  BUNDLE_ROOT="${SCRIPT_DIR}"
  DEPLOY_SCRIPT="${SCRIPT_DIR}/deploy.sh"
  DEFAULT_STATE_ROOT="${SCRIPT_DIR}/release-state"
  BUNDLE_IMAGE_ENV="${SCRIPT_DIR}/image.env"
else
  REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  BUNDLE_ROOT=""
  DEPLOY_SCRIPT="${SCRIPT_DIR}/deploy.sh"
  DEFAULT_STATE_ROOT="${REPO_ROOT}/release-state"
  BUNDLE_IMAGE_ENV=""
fi

# --- arguments -------------------------------------------------------------------
ACTION=""
ENVIRONMENT=""
IMAGE_TAG=""
ROLLBACK_TO=""
REPLICAS=""
DEPS_MODE_FLAG=""
OPT_ENV_FILE=""
HEALTH_TIMEOUT="${SDKWORK_RELEASE_HEALTH_TIMEOUT:-180}"
AUTO_ROLLBACK="1"
SKIP_VERIFY="0"
LOCK_TIMEOUT="${SDKWORK_RELEASE_LOCK_TIMEOUT:-120}"
DRY_RUN="0"

if [ $# -eq 0 ]; then usage; fi
case "$1" in
  deploy|rollback|status|history|verify|versions) ACTION="$1"; shift ;;
  -h|--help) usage ;;
  *) die "first argument must be deploy|rollback|status|history|verify|versions (got: $1)" ;;
esac

while [ $# -gt 0 ]; do
  case "$1" in
    --environment)     ENVIRONMENT="$2"; shift 2 ;;
    --image-tag)       IMAGE_TAG="$2"; shift 2 ;;
    --to)              ROLLBACK_TO="$2"; shift 2 ;;
    --replicas)        REPLICAS="$2"; shift 2 ;;
    --embedded)        DEPS_MODE_FLAG="embedded"; shift ;;
    --external)        DEPS_MODE_FLAG="external"; shift ;;
    --env-file)        OPT_ENV_FILE="$2"; shift 2 ;;
    --health-timeout)  HEALTH_TIMEOUT="$2"; shift 2 ;;
    --no-auto-rollback) AUTO_ROLLBACK="0"; shift ;;
    --skip-verify)     SKIP_VERIFY="1"; shift ;;
    --lock-timeout)    LOCK_TIMEOUT="$2"; shift 2 ;;
    --dry-run)         DRY_RUN="1"; shift ;;
    -h|--help)         usage ;;
    *)                 die "unsupported option: $1 (see --help)" ;;
  esac
done

[ -n "${ENVIRONMENT}" ] || die "--environment is required (development|test|staging|demo|production)"
case "${ENVIRONMENT}" in
  development|test|staging|demo|production) ;;
  *) die "unsupported environment: ${ENVIRONMENT} (development|test|staging|demo|production)" ;;
esac
[ -f "${DEPLOY_SCRIPT}" ] || die "deploy.sh missing beside release.sh: ${DEPLOY_SCRIPT}"

if [ -n "${OPT_ENV_FILE}" ]; then
  ENV_FILE="${OPT_ENV_FILE}"
  [ -f "${ENV_FILE}" ] || die "external env file missing: ${ENV_FILE}"
  STATE_ROOT="${SDKWORK_RELEASE_STATE_DIR:-$(cd "$(dirname "${ENV_FILE}")" && pwd)/release-state}"
else
  STATE_ROOT="${SDKWORK_RELEASE_STATE_DIR:-${DEFAULT_STATE_ROOT}}"
  if [ "${BUNDLE_ROOT}" != "" ]; then
    ENV_FILE="${BUNDLE_ROOT}/env/${ENVIRONMENT}.env"
  else
    ENV_FILE="${REPO_ROOT}/docker/env/${ENVIRONMENT}.env"
  fi
fi
ENV_EXAMPLE="${ENV_FILE%.env}.env.example"
if [ ! -f "${ENV_FILE}" ]; then
  [ -f "${ENV_EXAMPLE}" ] || die "env file missing and no example: ${ENV_EXAMPLE}"
  info "env file missing; deploy.sh will create it from the example on first apply"
fi
ENV_DIR="$(cd "$(dirname "${ENV_FILE}")" 2>/dev/null && pwd || dirname "${ENV_FILE}")"
ENV_STATE_DIR="${STATE_ROOT}/${ENVIRONMENT}"
LEDGER_FILE="${ENV_STATE_DIR}/ledger.jsonl"
CURRENT_FILE="${ENV_STATE_DIR}/current.json"

# Read one KEY=value from the env file when it exists (no side effects).
env_key() {
  if [ -f "${ENV_FILE}" ]; then
    sed -n "s/^${1}=//p" "${ENV_FILE}" | tail -1 | tr -d '\r'
  fi
}

now_iso() { date -u +%Y-%m-%dT%H:%M:%SZ; }

# --- docker helpers ----------------------------------------------------------------
image_exists() { docker image inspect "$1" >/dev/null 2>&1; }
image_id()     { docker image inspect --format '{{.Id}}' "$1" 2>/dev/null || true; }

resolve_image_base() {
  # Bundle deployments always use the canonical registry name (image.tar.gz
  # restores it verbatim); repo-layout deployments honor the env-file
  # GATEWAY_IMAGE repository (same rule as deploy.sh).
  if [ "${BUNDLE_ROOT}" != "" ]; then
    printf '%s' "registry.sdkwork.com/apps/sdkwork-api-cloud-gateway"
    return
  fi
  local env_image base
  env_image="$(env_key GATEWAY_IMAGE)"
  case "${env_image}" in
    *:*) base="${env_image%:*}" ;;
    *)   base="${env_image}" ;;
  esac
  printf '%s' "${base:-registry.sdkwork.com/apps/sdkwork-api-cloud-gateway}"
}

resolve_image_tag() {
  # Priority: --image-tag > bundle image.env > env-file GATEWAY_IMAGE tag.
  if [ -n "${IMAGE_TAG}" ]; then printf '%s' "${IMAGE_TAG}"; return; fi
  if [ -n "${BUNDLE_IMAGE_ENV}" ] && [ -f "${BUNDLE_IMAGE_ENV}" ]; then
    local tag
    tag="$(sed -n 's/^SDKWORK_API_CLOUD_GATEWAY_IMAGE_TAG=//p' "${BUNDLE_IMAGE_ENV}" | tail -1 | tr -d '\r')"
    if [ -n "${tag}" ]; then printf '%s' "${tag}"; return; fi
  fi
  local tag
  tag="$(env_key GATEWAY_IMAGE | sed 's/.*://')"
  printf '%s' "${tag:-0.1.0}"
}

IMAGE_BASE="$(resolve_image_base)"

# --- state helpers --------------------------------------------------------------------
mkdir_state() { mkdir -p "${ENV_STATE_DIR}"; }

# Escape a string for safe embedding in a JSON double-quoted field (the
# ledger is machine-parsed: an unescaped quote or backslash in a note must
# never corrupt the line).
json_escape() { printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' | tr -d '\000-\010\013\014\016-\037'; }

# Per-environment release lock (mkdir is atomic on POSIX). Mutating actions
# only: concurrent deploy/rollback fail after the wait budget instead of
# interleaving compose operations.
acquire_release_lock() {
  mkdir_state
  local waited=0
  while ! mkdir "${ENV_STATE_DIR}/.lock" 2>/dev/null; do
    if [ "${waited}" -ge "${LOCK_TIMEOUT}" ]; then
      die "another release operation holds ${ENV_STATE_DIR}/.lock (owner: $(cat "${ENV_STATE_DIR}/.lock/owner" 2>/dev/null || echo unknown)); after confirming no release is running, remove the stale directory"
    fi
    sleep 2; waited=$((waited + 2))
  done
  printf '%s %s\n' "$(now_iso)" "${SUDO_USER:-${USER:-unknown}}" > "${ENV_STATE_DIR}/.lock/owner"
  trap 'rm -rf "${ENV_STATE_DIR}/.lock"' EXIT
}

append_ledger() {
  # append_ledger <action> <fromVersion> <toVersion> <imageRef> <imageId> <result> <note>
  mkdir_state
  printf '{"ts":"%s","environment":"%s","action":"%s","fromVersion":%s,"toVersion":%s,"imageRef":"%s","imageId":"%s","result":"%s","note":"%s","user":"%s"}\n' \
    "$(now_iso)" "$(json_escape "${ENVIRONMENT}")" "$(json_escape "$1")" "$2" "$3" "$(json_escape "$4")" "$(json_escape "$5")" "$(json_escape "$6")" "$(json_escape "$7")" "$(json_escape "${SUDO_USER:-${USER:-unknown}}")" \
    >> "${LEDGER_FILE}"
}

# JSON string or null (arguments are already quoted or the literal null).
json_or_null() { if [ -n "$1" ]; then printf '"%s"' "$1"; else printf 'null'; fi; }

write_current() {
  # write_current <version> <imageRef> <imageId> <replicas> <hostPortBase> <depsMode>
  mkdir_state
  printf '{"version":"%s","imageRef":"%s","imageId":"%s","deployedAt":"%s","replicas":%s,"hostPortBase":%s,"depsMode":"%s"}\n' \
    "$1" "$2" "$3" "$(now_iso)" "$4" "$5" "$6" > "${CURRENT_FILE}"
}

read_current_field() {
  # best-effort scalar extraction without jq (current.json is script-written)
  [ -f "${CURRENT_FILE}" ] || return 0
  sed -n "s/.*\"$1\":\"\{0,1\}\([^,\"}]*\)\"\{0,1\}.*/\1/p" "${CURRENT_FILE}" | head -1
}

# --- fleet topology (kept in lockstep with deploy.sh) -----------------------------------
resolve_topology() {
  if [ -z "${REPLICAS}" ]; then REPLICAS="$(env_key GATEWAY_REPLICAS)"; fi
  REPLICAS="${REPLICAS:-1}"
  case "${REPLICAS}" in ''|*[!0-9]*) die "replicas must be a positive integer" ;; esac
  HOST_PORT_BASE="$(env_key GATEWAY_HOST_PORT)"
  HOST_PORT_BASE="${HOST_PORT_BASE:-3910}"
  case "${HOST_PORT_BASE}" in ''|*[!0-9]*) die "GATEWAY_HOST_PORT must be a positive integer" ;; esac
  DEPS_MODE="${DEPS_MODE_FLAG:-$(env_key GATEWAY_DEPS_MODE)}"
  DEPS_MODE="${DEPS_MODE:-external}"
  GATEWAY_CONTAINER_PORT="$(env_key GATEWAY_CONTAINER_PORT)"
  GATEWAY_CONTAINER_PORT="${GATEWAY_CONTAINER_PORT:-8080}"
}

instance_port() { echo $((HOST_PORT_BASE + ($1 - 1) * 10)); }

# --- health probe ------------------------------------------------------------------------
probe_once() {
  # probe_once <port> -> 0 when /healthz answers 2xx
  if command -v curl >/dev/null 2>&1; then
    curl -fsS -m 5 -o /dev/null "http://127.0.0.1:$1/healthz" 2>/dev/null
  else
    wget -q -T 5 -O /dev/null "http://127.0.0.1:$1/healthz" 2>/dev/null
  fi
}

probe_fleet() {
  # probe_fleet <label> — every instance must answer /healthz within budget
  local label="$1" index port waited ok
  for index in $(seq 1 "${REPLICAS}"); do
    port="$(instance_port "${index}")"
    waited=0; ok=0
    while [ "${waited}" -lt "${HEALTH_TIMEOUT}" ]; do
      if probe_once "${port}"; then ok=1; break; fi
      sleep 5; waited=$((waited + 5))
    done
    if [ "${ok}" -eq 1 ]; then
      info "[${label}] instance ${index} healthy on port ${port} (waited ${waited}s)"
    else
      warn "[${label}] instance ${index} NOT healthy on port ${port} after ${HEALTH_TIMEOUT}s"
      return 1
    fi
  done
  return 0
}

# --- deploy.sh passthrough -----------------------------------------------------------------
deploy_sh() {
  # deploy_sh <deploy-args...> — runs the co-located deploy.sh
  local args=(--environment "${ENVIRONMENT}")
  if [ -n "${OPT_ENV_FILE}" ]; then args+=(--env-file "${OPT_ENV_FILE}"); fi
  "$DEPLOY_SCRIPT" "${args[@]}" "$@"
}

# --- running image drift check ---------------------------------------------------------------
running_images() {
  # Prints "<project> <imageRef> <imageId>" for the gateway container of every
  # instance project of the environment. The image ID is compared too: a
  # re-tagged image (same name:tag, different content) must show up as drift.
  local projects
  if ! command -v docker >/dev/null 2>&1; then return 0; fi
  projects="$(docker ps --filter "label=com.docker.compose.project" --format '{{.Label "com.docker.compose.project"}}' 2>/dev/null \
    | grep -E "^sdkwork-api-cloud-gateway-${ENVIRONMENT}-i[0-9]+$" | sort -u || true)"
  local p cid img iid
  for p in ${projects}; do
    img="$(docker ps --filter "label=com.docker.compose.project=${p}" --filter "label=com.docker.compose.service=gateway" --format '{{.Image}}' 2>/dev/null | head -1 || true)"
    cid="$(docker ps -q --filter "label=com.docker.compose.project=${p}" --filter "label=com.docker.compose.service=gateway" 2>/dev/null | head -1 || true)"
    iid=""
    if [ -n "${cid}" ]; then
      iid="$(docker inspect --format '{{.Image}}' "${cid}" 2>/dev/null || true)"
    fi
    printf '%s %s %s\n' "${p}" "${img}" "${iid}"
  done
}

action_versions() {
  # List locally available versions of the gateway image — the candidate pool
  # for deploy/rollback targets on this host.
  local ref
  ref="${IMAGE_BASE}"
  if ! command -v docker >/dev/null 2>&1; then die "docker is required"; fi
  info "local image versions for ${ref}:"
  docker images --filter "reference=${ref}:*" --format '{{.Tag}}\t{{.ID}}\t{{.CreatedSince}}' 2>/dev/null | sort
  local current
  current="$(read_current_field version)"
  if [ -n "${current}" ]; then
    info "deployed: ${current} (see status)"
  fi
}

# --- actions ------------------------------------------------------------------------------------
previous_successful_version() {
  # Most recent ledger success whose imageTag differs from $1, newest first.
  [ -f "${LEDGER_FILE}" ] || return 0
  awk -v cur="$1" '
    /"result":"success"/ {
      match($0, /"toVersion":"[^"]*"/); tag=substr($0, RSTART+13, RLENGTH-14);
      match($0, /"ts":"[^"]*"/);        ts=substr($0, RSTART+6, RLENGTH-7);
      if (tag != cur && !(tag in seen)) { order[++n]=ts; ver[ts]=tag; seen[tag]=1 }
    }
    END { for (i=n; i>=1; i--) print ver[order[i]] }
  ' "${LEDGER_FILE}" | head -1
}

action_deploy() {
  local target_tag target_ref target_id from_version probe_ok=1
  target_tag="${IMAGE_TAG}"
  [ -n "${target_tag}" ] || target_tag="$(resolve_image_tag)"
  target_ref="${IMAGE_BASE}:${target_tag}"

  from_version="$(read_current_field version)"
  if [ -n "${from_version}" ] && [ "${from_version}" = "${target_tag}" ]; then
    info "target version equals the deployed version (${target_tag}); applying in place (idempotent re-apply)"
  fi

  if [ "${DRY_RUN}" = "1" ]; then
    info "dry-run plan:"
    info "  action: deploy ${from_version:-<none>} -> ${target_tag}"
    info "  image: ${target_ref}"
    info "  deploy.sh: $(basename "${DEPLOY_SCRIPT}") --environment ${ENVIRONMENT} $( [ -n "${DEPS_MODE_FLAG}" ] && printf -- '--%s' "${DEPS_MODE_FLAG}" )$( [ -n "${REPLICAS}" ] && printf ' --replicas %s' "${REPLICAS}" ) --image-tag ${target_tag}"
    info "  health gate: /healthz on ports ${HOST_PORT_BASE} (+10 per instance), budget ${HEALTH_TIMEOUT}s"
    info "  on failure: $([ "${AUTO_ROLLBACK}" = 1 ] && echo "auto-rollback to ${from_version:-<none>}" || echo "keep failed deployment (--no-auto-rollback)")"
    info "  state: ${ENV_STATE_DIR}"
    return 0
  fi

  if ! image_exists "${target_ref}"; then
    if [ -n "${BUNDLE_ROOT}" ] && [ -f "${BUNDLE_ROOT}/image.tar.gz" ]; then
      # The tarball restores exactly one tag (the bundle's image.env). If the
      # target differs, deploy.sh would load the tarball and then still fail;
      # surface the mismatch before touching the fleet.
      local bundle_tag=""
      if [ -f "${BUNDLE_IMAGE_ENV}" ]; then
        bundle_tag="$(sed -n 's/^SDKWORK_API_CLOUD_GATEWAY_IMAGE_TAG=//p' "${BUNDLE_IMAGE_ENV}" | tail -1 | tr -d '\r')"
      fi
      if [ -n "${bundle_tag}" ] && [ "${bundle_tag}" != "${target_tag}" ]; then
        die "image ${target_ref} missing locally and this bundle carries ${IMAGE_BASE}:${bundle_tag} (image.tar.gz), not ${target_tag} — run from the bundle of the target version, or build/pull ${target_ref} first"
      fi
      if [ "${SKIP_VERIFY}" = "1" ]; then
        warn "skipping bundle image.tar.gz sha256 integrity check (--skip-verify)"
      else
        local sumfile="${BUNDLE_ROOT}/image.sha256" expected actual
        if [ -f "${sumfile}" ]; then
          expected="$(awk '{print $1}' "${sumfile}")"
          actual="$(sha256sum "${BUNDLE_ROOT}/image.tar.gz" | awk '{print $1}')"
          [ "${expected}" = "${actual}" ] || die "bundle integrity check FAILED: image.sha256 (${expected}) does not match image.tar.gz (${actual}) — the bundle may be corrupted or tampered"
          info "bundle integrity check passed (sha256 ${expected})"
        else
          warn "no image.sha256 beside image.tar.gz; integrity check skipped"
        fi
      fi
      info "image ${target_ref} missing locally; deploy.sh will load it from the bundle image.tar.gz"
    else
      die "image ${target_ref} not found locally and no bundle image.tar.gz available — build/pull it first"
    fi
  fi

  info "deploy ${from_version:-<initial>} -> ${target_tag} (${target_ref})"
  local fail_note="health gate failed"
  if ! deploy_sh --image-tag "${target_tag}" \
      $( [ -n "${DEPS_MODE_FLAG}" ] && printf -- '--%s' "${DEPS_MODE_FLAG}" ) \
      $( [ -n "${REPLICAS}" ] && printf ' --replicas %s' "${REPLICAS}" ); then
    fail_note="deploy.sh apply failed"
    warn "deploy.sh apply failed"
    probe_ok=0
  else
    probe_fleet "post-deploy" || probe_ok=0
  fi

  if [ "${probe_ok}" -eq 1 ]; then
    target_id="$(image_id "${target_ref}")"
    write_current "${target_tag}" "${target_ref}" "${target_id}" "${REPLICAS}" "${HOST_PORT_BASE}" "${DEPS_MODE}"
    append_ledger "deploy" "$(json_or_null "${from_version}")" "\"${target_tag}\"" "${target_ref}" "${target_id}" "success" "health gate passed"
    info "release ${target_tag} is live on ${ENVIRONMENT} (ledger: ${LEDGER_FILE})"
    return 0
  fi

  append_ledger "deploy" "$(json_or_null "${from_version}")" "\"${target_tag}\"" "${target_ref}" "$(image_id "${target_ref}")" "failed" "${fail_note}"
  if [ "${AUTO_ROLLBACK}" = "1" ] && [ -n "${from_version}" ]; then
    if [ "${from_version}" = "${target_tag}" ]; then
      die "deploy of ${target_tag} failed and the previous version is also ${from_version} — auto-rollback is a no-op here; fix the root cause (logs: deploy.sh --environment ${ENVIRONMENT} --logs 1) and re-run deploy"
    fi
    warn "health gate failed — rolling back to ${from_version}"
    action_rollback_core "${from_version}" "auto-rollback after failed deploy of ${target_tag}"
    die "deploy of ${target_tag} failed and was rolled back to ${from_version} (see ${LEDGER_FILE})"
  fi
  die "deploy of ${target_tag} failed (health gate) — deployment left in place (--no-auto-rollback)"
}

action_rollback_core() {
  # action_rollback_core <version> <note> — the shared rollback executor
  local target="$1" note="$2" current_ref current_id target_ref target_id from_version
  from_version="$(read_current_field version)"
  target_ref="${IMAGE_BASE}:${target}"
  current_id="$(read_current_field imageId)"
  [ -n "${current_id}" ] || current_id="$(image_id "${IMAGE_BASE}:${from_version:-unknown}")"

  if ! image_exists "${target_ref}"; then
    die "rollback image ${target_ref} missing locally — load it (docker load) or point SDKWORK_RELEASE_STATE_DIR at the state of a bundle that carries it"
  fi

  info "rollback ${from_version:-unknown} -> ${target} (${target_ref})"
  if ! deploy_sh --image-tag "${target}" \
      $( [ -n "${DEPS_MODE_FLAG}" ] && printf -- '--%s' "${DEPS_MODE_FLAG}" ) \
      $( [ -n "${REPLICAS}" ] && printf ' --replicas %s' "${REPLICAS}" ); then
    append_ledger "rollback" "$(json_or_null "${from_version}")" "\"${target}\"" "${target_ref}" "$(image_id "${target_ref}")" "failed" "${note}: deploy.sh apply failed"
    die "rollback apply failed — inspect deploy.sh logs immediately"
  fi
  if ! probe_fleet "post-rollback"; then
    append_ledger "rollback" "$(json_or_null "${from_version}")" "\"${target}\"" "${target_ref}" "$(image_id "${target_ref}")" "failed" "${note}: health gate failed"
    die "rollback to ${target} applied but the health gate failed — escalate (logs, previous bundle)"
  fi
  target_id="$(image_id "${target_ref}")"
  write_current "${target}" "${target_ref}" "${target_id}" "${REPLICAS}" "${HOST_PORT_BASE}" "${DEPS_MODE}"
  append_ledger "rollback" "$(json_or_null "${from_version}")" "\"${target}\"" "${target_ref}" "${target_id}" "success" "${note}"
  info "rolled back to ${target}; ${from_version:-unknown} stays in the ledger for a later forward rollback"
}

action_rollback() {
  local target
  if [ -n "${ROLLBACK_TO}" ]; then
    target="${ROLLBACK_TO}"
  else
    target="$(previous_successful_version "$(read_current_field version)")"
    [ -n "${target}" ] || die "no previous successful version found in ${LEDGER_FILE} — use --to <version>"
  fi
  if [ "${DRY_RUN}" = "1" ]; then
    info "dry-run plan: rollback $(read_current_field version) -> ${target} (${IMAGE_BASE}:${target}), then probe ports ${HOST_PORT_BASE}+"
    return 0
  fi
  action_rollback_core "${target}" "explicit rollback"
}

action_status() {
  local version deployed_at image_ref="" image_id="" replicas base port index running drift=0
  version="$(read_current_field version)"
  if [ -z "${version}" ]; then
    info "environment ${ENVIRONMENT}: no release recorded in ${CURRENT_FILE}"
    info "hint: run '$(basename "$0")' deploy --environment ${ENVIRONMENT} to record the first release"
  else
    deployed_at="$(read_current_field deployedAt)"
    image_ref="$(read_current_field imageRef)"
    image_id="$(read_current_field imageId)"
    info "environment ${ENVIRONMENT}"
    info "  deployed version : ${version} (at ${deployed_at})"
    info "  recorded image   : ${image_ref} ${image_id}"
  fi
  resolve_topology
  info "  topology         : ${REPLICAS} instance(s), host ports ${HOST_PORT_BASE} (+10 per instance) -> container ${GATEWAY_CONTAINER_PORT}, deps=${DEPS_MODE}"
  for index in $(seq 1 "${REPLICAS}"); do
    port="$(instance_port "${index}")"
    if probe_once "${port}"; then
      info "  instance ${index} : port ${port} HEALTHY"
    else
      warn "  instance ${index} : port ${port} UNREACHABLE (/healthz)"
    fi
  done
  if [ "${DRY_RUN}" = "1" ]; then return 0; fi
  local project img iid
  while read -r project img iid; do
    [ -n "${project}" ] || continue
    if [ -n "${image_ref}" ] && [ "${img}" != "${image_ref}" ]; then
      warn "  drift: ${project} runs ${img} but the ledger records ${image_ref}"
      drift=1
    fi
    if [ -n "${image_id}" ] && [ -n "${iid}" ] && [ "${iid}" != "${image_id}" ]; then
      warn "  drift: ${project} runs image ${iid} but the ledger records ${image_id} (same tag, different content?)"
      drift=1
    fi
  done < <(running_images)
  [ "${drift}" = 0 ] && info "  drift            : none (containers match the recorded release)" || true
}

action_history() {
  if [ ! -f "${LEDGER_FILE}" ]; then
    info "no release history yet: ${LEDGER_FILE}"
    return 0
  fi
  info "release ledger for ${ENVIRONMENT} (${LEDGER_FILE}):"
  awk '{
    ts=""; action=""; from="-"; to="-"; result=""; note="";
    if (match($0,/"ts":"[^"]*"/))          { ts=substr($0,RSTART+6,RLENGTH-7) }
    if (match($0,/"action":"[^"]*"/))      { action=substr($0,RSTART+10,RLENGTH-11) }
    if (match($0,/"fromVersion":"[^"]*"/)) { from=substr($0,RSTART+15,RLENGTH-16) }
    if (match($0,/"toVersion":"[^"]*"/))   { to=substr($0,RSTART+13,RLENGTH-14) }
    if (match($0,/"result":"[^"]*"/))      { result=substr($0,RSTART+10,RLENGTH-11) }
    if (match($0,/"note":"[^"]*"/))        { note=substr($0,RSTART+8,RLENGTH-9) }
    printf "  %s  %-8s  %-10s -> %-10s  %-7s  %s\n", ts, action, from, to, result, note
  }' "${LEDGER_FILE}"
}

action_verify() {
  resolve_topology
  info "verifying ${ENVIRONMENT}: ${REPLICAS} instance(s) on ports ${HOST_PORT_BASE}+"
  if probe_fleet "verify"; then
    info "all instances healthy"
    return 0
  fi
  die "health verification failed"
}

resolve_topology
case "${ACTION}" in
  deploy)
    [ "${DRY_RUN}" = "1" ] || acquire_release_lock
    action_deploy
    ;;
  rollback)
    [ "${DRY_RUN}" = "1" ] || acquire_release_lock
    action_rollback
    ;;
  status)   action_status ;;
  history)  action_history ;;
  verify)   action_verify ;;
  versions) action_versions ;;
esac

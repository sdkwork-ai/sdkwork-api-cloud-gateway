#!/usr/bin/env bash
# ============================================================================
# remote-deploy.sh — build locally, push to a remote Ubuntu host over SSH
# key-based authentication, and deploy there.
#
# Pipeline (DEPLOYMENT_SPEC.md §6 / PNPM_SCRIPT_SPEC.md §4.4):
#   1. verify passwordless SSH (ssh -o BatchMode=yes)
#   2. package the unified install bundle locally (or reuse --bundle)
#   3. push the bundle to <remote-root> on the remote host
#   4. materialize the per-environment config at the Ubuntu config root
#      <config-root>/<env>.env — the SINGLE file every instance of that
#      environment reads (APPLICATION_DEPLOY_LAYOUT_SPEC §3: config root
#      /etc/sdkwork/api-gateway/ for applicationCode api-gateway). Custom
#      database passwords and all deployment inputs are edited THERE.
#   5. run the remote bundle deploy.sh with --env-file pointing at that file
#   6. health-check instance 1
#
# Usage:
#   bash scripts/docker/remote-deploy.sh --host deploy@1.2.3.4 \
#       --environment production --replicas 3
#
# Options:
#   --host <user@host>      remote Ubuntu host (required)
#   --ssh-key <path>        SSH private key (default ~/.ssh/id_ed25519)
#   --ssh-port <port>       SSH port (default 22)
#   --environment <env>     development|test|staging|production (default development)
#   --replicas <N>          gateway instances (default 1)
#   --bundle <dir>          reuse an existing bundle directory instead of building
#   --image-tag <tag>       image tag used when building locally
#   --remote-root <dir>     remote bundle install root
#                           (default /opt/sdkwork/deploy/sdkwork-api-cloud-gateway)
#   --config-root <dir>     remote per-environment config directory
#                           (default /etc/sdkwork/api-gateway/docker, per
#                           APPLICATION_DEPLOY_LAYOUT_SPEC §3)
#   --sync-env              overwrite the remote env file with the local one
#                           (default: keep the remote file — remote edits such
#                           as the database password win)
#   --down | --stop | --start | --restart | --ps | --logs [N]
#                           forward the action to the remote deploy.sh
#   --dry-run               print the resolved commands only
#   -h, --help              show this help
#
# Passwordless SSH: run `ssh-copy-id <user>@<host>` once. For the default
# config root under /etc, the remote user needs passwordless sudo
# (`sudo -n`); pass --config-root to a user-writable directory otherwise.
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
LOG_PREFIX="[sdkwork-gateway-remote]"

info() { printf '%s %s\n' "$LOG_PREFIX" "$*"; }
die()  { printf '%s ERROR: %s\n' "$LOG_PREFIX" "$*" >&2; exit 1; }
usage() { sed -n '2,/^set -euo pipefail$/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0; }

HOST=""
SSH_KEY="${SDKWORK_SSH_KEY:-${HOME}/.ssh/id_ed25519}"
SSH_PORT="${SDKWORK_SSH_PORT:-22}"
ENVIRONMENT="development"
REPLICAS="1"
BUNDLE=""
IMAGE_TAG=""
REMOTE_ROOT="/opt/sdkwork/deploy/sdkwork-api-cloud-gateway"
CONFIG_ROOT="/etc/sdkwork/api-gateway/docker"
SYNC_ENV="0"
ACTION="apply"
LOG_INSTANCE="1"
DRY_RUN="0"

while [ $# -gt 0 ]; do
  case "$1" in
    # `pnpm run <script> -- --flag` forwards a literal `--` on some pnpm
    # versions/platforms; ignore it.
    --) shift ;;
    --host)        HOST="$2"; shift 2 ;;
    --ssh-key)     SSH_KEY="$2"; shift 2 ;;
    --ssh-port)    SSH_PORT="$2"; shift 2 ;;
    --environment) ENVIRONMENT="$2"; shift 2 ;;
    --replicas)    REPLICAS="$2"; shift 2 ;;
    --bundle)      BUNDLE="$2"; shift 2 ;;
    --image-tag)   IMAGE_TAG="$2"; shift 2 ;;
    --remote-root) REMOTE_ROOT="$2"; shift 2 ;;
    --config-root) CONFIG_ROOT="$2"; shift 2 ;;
    --sync-env)    SYNC_ENV="1"; shift ;;
    --down)        ACTION="down"; shift ;;
    --stop)        ACTION="stop"; shift ;;
    --start)       ACTION="start"; shift ;;
    --restart)     ACTION="restart"; shift ;;
    --ps)          ACTION="ps"; shift ;;
    --logs)        ACTION="logs"; LOG_INSTANCE="${2:-1}"; case "${2:-}" in ''|*[!0-9]*) shift ;; *) shift 2 ;; esac ;;
    --dry-run)     DRY_RUN="1"; shift ;;
    -h|--help)     usage ;;
    *)             die "unsupported option: $1 (see --help)" ;;
  esac
done

[ -n "${HOST}" ] || die "--host <user@host> is required"
case "${ENVIRONMENT}" in
  development|test|staging|production) ;;
  *) die "unsupported environment: ${ENVIRONMENT} (development|test|staging|production)" ;;
esac
case "${REPLICAS}" in ''|*[!0-9]*) die "--replicas must be a positive integer" ;; esac

SSH_BASE=(ssh -o BatchMode=yes -o ConnectTimeout=10 -p "${SSH_PORT}")
[ -f "${SSH_KEY}" ] && SSH_BASE+=(-i "${SSH_KEY}")

remote() {
  # remote <command...> — run a command on the remote host.
  info "remote: $*"
  if [ "${DRY_RUN}" != "1" ]; then
    "${SSH_BASE[@]}" "${HOST}" "$@"
  fi
}

# --- 1. passwordless SSH probe ---------------------------------------------------
if [ "${DRY_RUN}" != "1" ]; then
  if ! "${SSH_BASE[@]}" "${HOST}" 'echo sdkwork-ssh-ok' >/dev/null 2>&1; then
    die "passwordless SSH failed for ${HOST}; run once: ssh-copy-id ${HOST} (and check --ssh-key / --ssh-port)"
  fi
  info "passwordless SSH ok: ${HOST}"
fi

# --- 2. local packaging ------------------------------------------------------------
if [ -z "${BUNDLE}" ]; then
  if [ "${DRY_RUN}" = "1" ]; then
    info "dry-run: would run node scripts/docker/package-install-bundle.mjs (tag ${IMAGE_TAG:-manifest version})"
    BUNDLE="dist/docker-install/sdkwork-api-cloud-gateway-install-<version>.bundle"
  else
    tag_args=()
    [ -n "${IMAGE_TAG}" ] && tag_args=(--tag "${IMAGE_TAG}")
    info "packaging the unified install bundle locally..."
    (cd "${REPO_ROOT}" && node scripts/docker/package-install-bundle.mjs "${tag_args[@]}")
    BUNDLE="$(ls -dt "${REPO_ROOT}"/dist/docker-install/sdkwork-api-cloud-gateway-install-*.bundle | head -1)"
    [ -n "${BUNDLE}" ] && [ -d "${BUNDLE}" ] || die "no bundle produced under dist/docker-install/"
  fi
else
  BUNDLE="$(cd "$(dirname "${BUNDLE}")" && pwd)/$(basename "${BUNDLE}")"
  [ -d "${BUNDLE}" ] || die "bundle directory missing: ${BUNDLE}"
fi
[ -f "${BUNDLE}/deploy.sh" ] || { [ "${DRY_RUN}" = "1" ] || die "bundle is missing deploy.sh: ${BUNDLE}"; }

# --- 3. remote directory preparation ------------------------------------------------
info "remote install root: ${REMOTE_ROOT}"
info "remote config root:  ${CONFIG_ROOT}"
remote "sudo -n mkdir -p '${REMOTE_ROOT}' '${CONFIG_ROOT}'" \
  || die "remote mkdir failed (need passwordless sudo for ${CONFIG_ROOT}); use --config-root <user-writable-dir>"

# --- 4. per-environment config at the Ubuntu config root -----------------------------
# All instances of the environment read this ONE file. The remote copy wins
# unless --sync-env, so operators edit the database password etc. remotely.
LOCAL_ENV="${REPO_ROOT}/docker/env/${ENVIRONMENT}.env"
[ -f "${LOCAL_ENV}" ] || LOCAL_ENV="${REPO_ROOT}/docker/env/${ENVIRONMENT}.env.example"
REMOTE_ENV="${CONFIG_ROOT}/${ENVIRONMENT}.env"

if [ "${SYNC_ENV}" = "1" ] || ! "${SSH_BASE[@]}" "${HOST}" "test -f '${REMOTE_ENV}'" >/dev/null 2>&1; then
  info "pushing env config -> ${REMOTE_ENV} (${LOCAL_ENV})"
  if [ "${DRY_RUN}" = "1" ]; then
    info "dry-run: sudo -n sh -c 'umask 077; cat > ${REMOTE_ENV}' < ${LOCAL_ENV}"
  else
    "${SSH_BASE[@]}" "${HOST}" "sudo -n sh -c 'umask 077; cat > \"${REMOTE_ENV}\"'" \
      < "${LOCAL_ENV}" || die "failed to write remote env config"
  fi
else
  info "remote env config already present (kept): ${REMOTE_ENV}"
  info "  edit it on the remote host to customize the database password etc.;"
  info "  re-run with --sync-env to overwrite it from the local env file"
fi

# --- 5. push the bundle ---------------------------------------------------------------
if [ "${DRY_RUN}" = "1" ]; then
  info "dry-run: would rsync/scp ${BUNDLE}/ -> ${HOST}:${REMOTE_ROOT}/"
else
  if command -v rsync >/dev/null 2>&1; then
    info "pushing bundle via rsync..."
    rsync -az --delete -e "ssh -p ${SSH_PORT}${SSH_KEY:+-i ${SSH_KEY}}" \
      "${BUNDLE}/" "${HOST}:${REMOTE_ROOT}/"
  else
    info "rsync not found; pushing bundle via scp..."
    scp_args=(-q -P "${SSH_PORT}")
    [ -f "${SSH_KEY}" ] && scp_args+=(-i "${SSH_KEY}")
    "${SSH_BASE[@]}" "${HOST}" "mkdir -p '${REMOTE_ROOT}'"
    scp "${scp_args[@]}" -r "${BUNDLE}/." "${HOST}:${REMOTE_ROOT}/"
  fi
fi

# --- 6. remote deploy -------------------------------------------------------------------
DEPLOY_ARGS=(--environment "${ENVIRONMENT}" --env-file "${REMOTE_ENV}")
if [ "${ACTION}" = "apply" ]; then
  DEPLOY_ARGS+=(--replicas "${REPLICAS}")
else
  # forward the requested action to the remote deploy.sh (--down/--ps/--logs/--stop/...)
  DEPLOY_ARGS+=("--${ACTION}")
  if [ "${ACTION}" = "logs" ]; then
    DEPLOY_ARGS+=("${LOG_INSTANCE}")
  fi
fi
if [ "${DRY_RUN}" = "1" ]; then
  info "dry-run: remote: cd ${REMOTE_ROOT} && bash deploy.sh ${DEPLOY_ARGS[*]}"
else
  info "running remote deploy.sh ${DEPLOY_ARGS[*]}"
  "${SSH_BASE[@]}" "${HOST}" "cd '${REMOTE_ROOT}' && bash deploy.sh ${DEPLOY_ARGS[*]}"
fi

# --- 7. health check (apply only) ---------------------------------------------------------
if [ "${ACTION}" = "apply" ] && [ "${DRY_RUN}" != "1" ]; then
  info "health check: ${HOST}:${ENVIRONMENT}"
  "${SSH_BASE[@]}" "${HOST}" \
    "port=\$(sed -n 's/^GATEWAY_HOST_PORT=//p' '${REMOTE_ENV}' | tail -1 | tr -d '\r'); port=\${port:-3910}; curl -fsS http://127.0.0.1:\${port}/healthz >/dev/null && echo \"healthz ok on :\${port}\" || { echo \"healthz FAILED on :\${port}\" >&2; exit 1; }"
fi

info "done: ${HOST} ${ENVIRONMENT} replicas=${REPLICAS}"

#!/usr/bin/env bash
# scripts/deploy/lib/common.sh
#
# Dependency-free shared helpers for the sdkwork-api-cloud-gateway deploy toolkit.
# Sourced (not executed) by every deploy script. Safe to source multiple times.
#
# Design goals (high cohesion / low coupling):
#   - no external dependencies beyond coreutils + openssl
#   - every script sources ONLY lib/common.sh + lib/environments.sh
#   - OS detection, logging, and privilege helpers live here so the per-task
#     scripts stay thin and single-purpose.

[[ -n "${SDKWORK_DEPLOY_LIB_COMMON:-}" ]] && return 0
SDKWORK_DEPLOY_LIB_COMMON=1

# ---- colorized logging (auto-disabled when stdout is not a TTY) ----
if [[ -t 1 ]]; then
  SDK_C_RESET=$'\033[0m'; SDK_C_RED=$'\033[31m'; SDK_C_GREEN=$'\033[32m'
  SDK_C_YELLOW=$'\033[33m'; SDK_C_BLUE=$'\033[34m'; SDK_C_BOLD=$'\033[1m'
else
  SDK_C_RESET=""; SDK_C_RED=""; SDK_C_GREEN=""; SDK_C_YELLOW=""; SDK_C_BLUE=""; SDK_C_BOLD=""
fi

log_info()  { printf '%s[INFO]%s %s\n'  "$SDK_C_BLUE"   "$SDK_C_RESET" "$*"; }
log_ok()    { printf '%s[ OK ]%s %s\n'  "$SDK_C_GREEN"  "$SDK_C_RESET" "$*"; }
log_warn()  { printf '%s[WARN]%s %s\n'  "$SDK_C_YELLOW" "$SDK_C_RESET" "$*"; }
log_error() { printf '%s[ERR ]%s %s\n'  "$SDK_C_RED"    "$SDK_C_RESET" "$*" >&2; }
log_step()  { printf '\n%s==>%s %s\n'   "$SDK_C_BOLD"   "$SDK_C_RESET" "$*"; }

die() { log_error "$*"; exit 1; }

# require_cmd <cmd> — die unless a command exists on PATH.
require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

# detect_os — linux | macos | windows
detect_os() {
  case "$(uname -s)" in
    Linux*)   echo linux ;;
    Darwin*)  echo macos ;;
    MINGW*|MSYS*|CYGWIN*) echo windows ;;
    *)        echo unknown ;;
  esac
}

# deploy_repo_root — absolute path to the gateway repository root.
# common.sh lives at <root>/scripts/deploy/lib/, so three ../ from dirname reaches <root>.
deploy_repo_root() {
  CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../" && pwd
}

# require_docker — ensure the docker CLI exists and the daemon is reachable.
require_docker() {
  require_cmd docker
  docker info >/dev/null 2>&1 || die "docker daemon not reachable; start Docker and retry"
}

is_root() { [[ "$(id -u)" -eq 0 ]]; }

# run_root <cmd...> — run a command with root privileges.
#   - already root:        run directly
#   - SDKWORK_SUDO_PASSWORD set: pipe it to `sudo -S` (non-interactive WSL/CI)
#   - otherwise:           interactive `sudo` (will prompt)
run_root() {
  if is_root; then
    "$@"
  elif [[ -n "${SDKWORK_SUDO_PASSWORD:-}" ]]; then
    printf '%s' "$SDKWORK_SUDO_PASSWORD" | sudo -S "$@"
  else
    sudo "$@"
  fi
}

# parse_env_selection <args...> — normalize a CLI env selection into the canonical
# list. Accepts explicit env names and the literal "all". Empty input -> all envs.
parse_env_selection() {
  local out=()
  for a in "$@"; do
    if [[ "$a" == "all" ]]; then out=($SDKWORK_DEPLOY_ENVS); break; fi
    out+=("$a")
  done
  [[ ${#out[@]} -eq 0 ]] && out=($SDKWORK_DEPLOY_ENVS)
  printf '%s\n' "${out[@]}"
}

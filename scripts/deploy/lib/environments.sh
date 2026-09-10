#!/usr/bin/env bash
# scripts/deploy/lib/environments.sh
#
# SINGLE SOURCE OF TRUTH for the deployment matrix.
# Every deploy script derives ports, databases, domains, and hostnames from here
# so the four environments never drift and there is no duplicated mapping anywhere.
#
# Convention (sdkwork-specs ENVIRONMENT_SPEC + DEPLOYMENT_SPEC):
#   - one host port per environment (no conflicts): dev 3910, test 3911, staging 3912, prod 3913
#   - one database identity per environment (db == schema == role)
#   - subdomain pattern:  api-dev / api-test / api-staging / api  (prod is the bare "api")
#   - every environment is served on all registered public domains (aliases in nginx)
#
# Edit ONLY this file when the matrix must change.

[[ -n "${SDKWORK_DEPLOY_LIB_ENV:-}" ]] && return 0
SDKWORK_DEPLOY_LIB_ENV=1

# shellcheck source=lib/common.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/common.sh"

# All lifecycle environments (order = host-port order). Five-environment canon
# (DOCKER_SPEC.md §3 / ENVIRONMENT_SPEC.md §5.1): demo joins the matrix on
# host port 3914 with workspace identity sdkwork_ai_demo.
SDKWORK_DEPLOY_ENVS="development test staging demo production"

# Public top-level domains the gateway is served on.
# MUST stay aligned with the registered base-domain family:
#   - deployments/webserver/server.<env>.toml (gateway nginx edge serverName)
#   - etc/sdkwork.deployment.config.json (cloudApiBaseUrl / publicHosts)
#   - specs/topology.spec.json (cloudPublicHosts)
#   - sdkwork-webserver deployments/webserver/server.<env>.toml (client edge)
# (ENVIRONMENT_SPEC §5.1.0.1: every environment serves every registered domain.)
SDKWORK_DEPLOY_DOMAINS="sdkwork.com birdcoder.com dtupay.com noaper.com sdkwork.cn birdcoder.cn dtupay.cn noaper.cn skubc.com skubc.cn zowalk.com zowalk.cn offer86.com offer86.cn 86offer.com 86offer.cn"

# env_prefix <env> -> subdomain prefix (empty for production)
env_prefix() {
  case "$1" in
    development) echo dev ;;
    test)        echo test ;;
    staging)     echo staging ;;
    demo)        echo demo ;;
    production)  echo "" ;;
    *) die "unknown environment: $1" ;;
  esac
}

# env_host_port <env> -> host port mapped 391x -> container :3900
env_host_port() {
  case "$1" in
    development) echo 3910 ;;
    test)        echo 3911 ;;
    staging)     echo 3912 ;;
    demo)        echo 3914 ;;
    production)  echo 3913 ;;
    *) die "unknown environment: $1" ;;
  esac
}

# env_db <env> -> workspace database / schema / role identity (sdkwork-specs)
env_db() {
  case "$1" in
    development) echo sdkwork_ai_dev ;;
    test)        echo sdkwork_ai_test ;;
    staging)     echo sdkwork_ai_staging ;;
    demo)        echo sdkwork_ai_demo ;;
    production)  echo sdkwork_ai_prod ;;
    *) die "unknown environment: $1" ;;
  esac
}

# env_canonical_host <env> -> the primary hostname (also the nginx conf filename)
env_canonical_host() {
  local p; p=$(env_prefix "$1")
  if [[ -z "$p" ]]; then echo "api.sdkwork.com"; else echo "api-$p.sdkwork.com"; fi
}

# env_server_names <env> -> space-separated nginx server_name list (all domains)
env_server_names() {
  local p out=""
  p=$(env_prefix "$1")
  for d in $SDKWORK_DEPLOY_DOMAINS; do
    if [[ -z "$p" ]]; then out="$out api.$d"; else out="$out api-$p.$d"; fi
  done
  # trim leading space
  echo "${out# }"
}

# env_allowed_hosts <env> -> comma list: public hosts + localhost/127.0.0.1:<port>
# Mirrors GATEWAY_ALLOWED_HOSTS in docker/env/<env>.env so the proxy Host passes.
env_allowed_hosts() {
  local port out=""
  port=$(env_host_port "$1")
  for name in $(env_server_names "$1"); do out="$out$name,"; done
  out="${out}localhost:${port},127.0.0.1:${port}"
  echo "$out"
}

# cert_repo_dir <env>  -> generated cert location inside the repo (gitignored)
cert_repo_dir() { echo "$(deploy_repo_root)/docker/certs/$1"; }

# cert_host_dir <env>  -> installed cert location on the Ubuntu host (nginx reads this)
cert_host_dir() { echo "/etc/ssl/sdkwork/api-gateway/$1"; }

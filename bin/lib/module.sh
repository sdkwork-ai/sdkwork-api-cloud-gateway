#!/usr/bin/env bash
# module.sh — sdkwork-api-cloud-gateway bin/ wiring (MODULE_BIN_SPEC.md §3).
# Only module identity, constants, and repo-command delegation live here.
# Every shared primitive (remote execution, packaging, checksums) comes from
# sdkwork-specs/bin/lib/sdkwork-common.sh.

SDKWORK_MODULE_ID="sdkwork-api-cloud-gateway"
SDKWORK_IMAGE_NAME="sdkwork-api-cloud-gateway"
SDKWORK_APP_TYPES="server"

# Operations wiring (OPERATIONS_SPEC.md): the compose service carrying the
# health probe, the probe path (the gateway publishes /readyz), and the
# per-environment host port it is published on (DOCKER_SPEC.md §3).
SDKWORK_PRIMARY_SERVICE="gateway"
SDKWORK_HEALTH_PATH="/readyz"
SDKWORK_CONFIG_ENV_SUBDIR="env"

sdkwork_module_bundle_dir() {
  printf '%s/scripts/docker/bundle' "${SDKWORK_MODULE_ROOT}"
}

# Install/upgrade consume the newest packaged install bundle
# (dist/docker-install/*, produced by the install-bundle packager) — a
# self-contained stage-2 artifact; fall back to the source bundle dir.
sdkwork_module_install_bundle_dir() {
  local d
  d="$(sdkwork_newest_install_bundle "${SDKWORK_MODULE_ROOT}/dist/docker-install")"
  if [[ -n "${d}" ]]; then printf '%s' "${d}"; return 0; fi
  sdkwork_module_bundle_dir
}

# Source-tree env dir; used for --dry-run rendering and by config.sh.
sdkwork_module_local_env_dir() {
  printf '%s/docker/env' "${SDKWORK_MODULE_ROOT}"
}

# Host port publishing the gateway container port 3900 for an environment.
sdkwork_module_health_port() {
  case "${1:-}" in
    development) printf '3910' ;;
    test)        printf '3911' ;;
    staging)     printf '3912' ;;
    demo)        printf '3914' ;;
    production)  printf '3913' ;;
    *)           printf '' ;;
  esac
}

# Authoritative image tag for the doctor image-drift check: the deployed
# bundle's image.env (deploy.sh resolves the bundle tag ahead of the env-file
# GATEWAY_IMAGE, which is decorative in bundle layout).
sdkwork_module_expected_image_tag() {
  local image_env tag
  image_env="$(sdkwork_remote_bundle_dir)/image.env"
  sdkwork_remote_file_exists "${SDKWORK_BIN_HOST}" "${image_env}" || return 0
  tag="$(sdkwork_remote_read_file "${SDKWORK_BIN_HOST}" "${image_env}" 2>/dev/null \
    | sed -n 's/^SDKWORK_API_CLOUD_GATEWAY_IMAGE_TAG=//p' | tail -1 | tr -d '\r')"
  [[ -n "${tag}" ]] && printf '%s' "${tag}"
  return 0
}

# Delegates to the repository's canonical deployment validator.
sdkwork_module_config_validate() {
  local env_file="$1"
  sdkwork_local_run node scripts/validate-docker-deployment.mjs --env-file "${env_file}"
}

# Backup wiring (OPERATIONS_SPEC.md §5): the gateway's PostgreSQL connection
# lives under the GATEWAY_POSTGRES_* keys and names the database with _DB.
sdkwork_module_backup_db_env_prefix() {
  printf 'GATEWAY_POSTGRES'
}

sdkwork_module_backup_db_name_suffix() {
  printf '_DB'
}

# ----------------------------------------------------------------------------
# Native installer packaging (apps-pkg-installer.sh, MODULE_BIN_SPEC.md §4.9)
# ----------------------------------------------------------------------------
sdkwork_installer_app() {
  local app_type="$1" platform="$2" environment="$3" profile="$4" out="$5" arch="$6" format="$7"
  case "${app_type}:${platform}" in
    server:linux)
      # Host-native channel: the gateway ships a self-contained server
      # package (scripts/package-server.mjs); there is no deb/rpm builder,
      # so other formats fall back to the container channel guidance.
      case "${format}" in
        ""|tar|tar.gz)
          sdkwork_local_run node scripts/package-server.mjs package \
            --deployment-profile "${profile}" --version "${SDKWORK_IMAGE_TAG_DEFAULT}"
          sdkwork_collect_artifact "${SDKWORK_MODULE_ROOT}/dist" "${out}" '*.tar.gz' ;;
        *)
          sdkwork_die "${SDKWORK_BIN_E_STATE}" \
            "sdkwork-api-cloud-gateway has no '${format}' installer builder; use the container channel: bin/docker-image.sh save && bin/docker-deploy.sh install --environment ${environment}" ;;
      esac ;;
    server:*)
      sdkwork_die "${SDKWORK_BIN_E_STATE}" \
        "sdkwork-api-cloud-gateway ships host-native packages for Linux only; '${platform}' server delivery is the container image channel (bin/docker-image.sh save)" ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_ENV}" \
        "app type '${app_type}' has no native installer channel (declared: ${SDKWORK_APP_TYPES})" ;;
  esac
}

# ----------------------------------------------------------------------------
# Container image (docker-image.sh build)
# ----------------------------------------------------------------------------
sdkwork_image_build() {
  local ref="$1" tag="$2"
  # The gateway container build takes the FULL image reference as --tag: its
  # own default ('sdkwork-api-cloud-gateway:local') carries no registry, so the
  # canonical ref is passed explicitly to satisfy DOCKER_SPEC.md §2.1.
  sdkwork_local_run pnpm build:container --tag "${ref}"
}

# ----------------------------------------------------------------------------
# Application build (apps-build.sh)
# ----------------------------------------------------------------------------
sdkwork_build_app() {
  local app_type="$1" environment="$2" profile="$3"
  case "${app_type}" in
    server)
      sdkwork_local_run cargo build --release ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_ENV}" "unsupported app type '${app_type}' (declared: ${SDKWORK_APP_TYPES})" ;;
  esac
}

# ----------------------------------------------------------------------------
# Application packaging (apps-package.sh)
# ----------------------------------------------------------------------------
sdkwork_package_app() {
  local app_type="$1" environment="$2" profile="$3" out="$4"
  case "${app_type}" in
    server)
      # package-server.mjs builds the release binaries itself (cargo) unless
      # --skip-build is passed; there is no --out option — the archive lands in
      # dist/server/ and is collected into --out afterwards.
      local version="${SDKWORK_IMAGE_TAG_DEFAULT:-0.1.0}"
      sdkwork_local_run node scripts/package-server.mjs package \
        --deployment-profile "${profile}" --version "${version}"
      sdkwork_collect_artifact "${SDKWORK_MODULE_ROOT}/dist/server" "${out}" \
        "*${version}*.tar.gz" '*.tar.gz' ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_ENV}" "unsupported app type '${app_type}'" ;;
  esac
}

# ----------------------------------------------------------------------------
# Application deployment (apps-deploy.sh)
# ----------------------------------------------------------------------------
sdkwork_deploy_app() {
  local app_type="$1" action="$2" environment="$3" profile="$4" host="$5"
  case "${app_type}" in
    server)
      local service="sdkwork-api-gateway.service"
      local staging="/opt/deploy/${SDKWORK_MODULE_ID}/packages"
      case "${action}" in
        status)
          sdkwork_service_status "${host}" "${service}" ;;
        install|upgrade)
          # The server package ships a single production config template
          # (RELEASE_CONFIG_TEMPLATES in package-server.mjs), so the host-native
          # channel is production-only; every other environment is delivered by
          # the container bundle behind sdkwork-webserver.
          [[ "${environment}" == "production" ]] || sdkwork_die "${SDKWORK_BIN_E_STATE}" \
            "the host-native server package covers production only (got '${environment}'); use bin/docker-deploy.sh install --environment ${environment}"
          local archive base stage_dir
          archive="$(sdkwork_require_artifact "${SDKWORK_MODULE_ROOT}/dist/server" '*.tar.gz' \
                     "run: bin/apps-package.sh server ${environment}")"
          base="$(basename "${archive}")"
          stage_dir="${base%.tar.gz}"
          # Requires a remote account with root privileges (systemd + /opt install).
          sdkwork_push_dir "${host}" "$(dirname "${archive}")" "${staging}"
          sdkwork_remote_in_dir "${host}" "${staging}" tar -xzf "${base}" -C "${staging}"
          sdkwork_remote_in_dir "${host}" "${staging}/${stage_dir}" \
            bash install/install-ubuntu.sh --environment production --deployment-profile "${profile}"
          sdkwork_service_enable_now "${host}" "${service}" ;;
        rollback)
          sdkwork_die "${SDKWORK_BIN_E_STATE}" \
            "the host-native server package keeps no install history; package the previous version (bin/apps-package.sh server ${environment}) and re-run 'bin/apps-deploy.sh server install ${environment}'" ;;
      esac ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_ENV}" "unsupported app type '${app_type}'" ;;
  esac
}

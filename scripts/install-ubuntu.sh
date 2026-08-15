#!/bin/sh
set -eu

environment=production
deployment_profile=standalone
destdir=
start_service=true

usage() {
  cat <<'EOF'
Usage: install-ubuntu.sh [options]

Options:
  --environment <development|test|staging|production>
  --deployment-profile <standalone|cloud>
  --destdir <path>   Stage under a package root without touching the host.
  --no-start         Install files without enabling or starting systemd.
  -h, --help         Show this help.
EOF
}

require_value() {
  if [ "$#" -lt 2 ] || [ -z "$2" ]; then
    echo "missing value for $1" >&2
    exit 2
  fi
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --environment)
      require_value "$@"
      environment=$2
      shift 2
      ;;
    --deployment-profile)
      require_value "$@"
      deployment_profile=$2
      shift 2
      ;;
    --destdir)
      require_value "$@"
      destdir=${2%/}
      shift 2
      ;;
    --no-start)
      start_service=false
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unsupported option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$environment" in
  development|test|staging|production) ;;
  *) echo "unsupported lifecycle environment: $environment" >&2; exit 2 ;;
esac
case "$deployment_profile" in
  standalone|cloud) ;;
  *) echo "unsupported deployment profile: $deployment_profile" >&2; exit 2 ;;
esac

if [ -z "$destdir" ] && [ "$(id -u)" -ne 0 ]; then
  echo "host installation requires root; run with sudo or use --destdir" >&2
  exit 1
fi
if [ -n "$destdir" ]; then
  case "$destdir" in
    /*) ;;
    *) echo "--destdir must be an absolute path" >&2; exit 2 ;;
  esac
fi

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
package_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
binary_source=$package_root/bin/sdkwork-api-cloud-gateway
unit_source=$script_dir/sdkwork-api-gateway.service
config_source=$package_root/config/sdkwork-api-cloud-gateway.$environment.toml

for required in "$binary_source" "$unit_source" "$config_source" \
  "$package_root/database-modules" "$package_root/sdkwork.app.config.json" \
  "$package_root/specs/component.spec.json"; do
  if [ ! -e "$required" ]; then
    echo "package is missing required install asset: $required" >&2
    exit 1
  fi
done

install_root=$destdir/opt/sdkwork/api-gateway
config_root=$destdir/etc/sdkwork/api-gateway
data_root=$destdir/var/lib/sdkwork/api-gateway
log_root=$destdir/var/log/sdkwork/api-gateway
cache_root=$destdir/var/cache/sdkwork/api-gateway
run_root=$destdir/run/sdkwork/api-gateway
unit_root=$destdir/usr/lib/systemd/system

install -d -m 0755 "$install_root/bin" "$install_root/database-modules" "$install_root/data" "$install_root/specs"
install -d -m 0750 "$config_root" "$data_root" "$log_root" "$cache_root" "$run_root"
install -d -m 0755 "$unit_root"
install -m 0755 "$binary_source" "$install_root/bin/sdkwork-api-cloud-gateway"
install -m 0644 "$package_root/sdkwork.app.config.json" "$install_root/sdkwork.app.config.json"
install -m 0644 "$package_root/specs/component.spec.json" "$install_root/specs/component.spec.json"
cp -R "$package_root/database-modules/." "$install_root/database-modules/"
if [ -d "$package_root/data" ]; then
  cp -R "$package_root/data/." "$install_root/data/"
fi

config_target=$config_root/sdkwork-api-cloud-gateway.toml
if [ ! -e "$config_target" ]; then
  install -m 0640 "$config_source" "$config_target"
fi

env_target=$config_root/sdkwork-api-cloud-gateway.env
if [ ! -e "$env_target" ]; then
  umask 027
  {
    echo "SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE=$deployment_profile"
    echo "SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT=$environment"
    echo "SDKWORK_ENVIRONMENT=$environment"
    echo "SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID=$deployment_profile.$environment"
    echo "SDKWORK_API_CLOUD_GATEWAY_RUNTIME_TARGET=server"
    echo "SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE=/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml"
    echo "SDKWORK_APP_ROOT=/opt/sdkwork/api-gateway"
    echo "SDKWORK_DATABASE_MODULES_ROOT=/opt/sdkwork/api-gateway/database-modules"
    echo "SDKWORK_MODELS_APP_ROOT=/opt/sdkwork/api-gateway/database-modules/sdkwork-models"
    echo "SDKWORK_MODELS_CATALOG_ROOT=/opt/sdkwork/api-gateway/data/sdkwork-models"
    echo "SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC=false"
  } > "$env_target"
  chmod 0640 "$env_target"
fi

install -m 0644 "$unit_source" "$unit_root/sdkwork-api-gateway.service"

if [ -z "$destdir" ]; then
  if ! getent group sdkwork >/dev/null 2>&1; then
    groupadd --system sdkwork
  fi
  if ! id sdkwork >/dev/null 2>&1; then
    useradd --system --gid sdkwork --home-dir /opt/sdkwork/api-gateway sdkwork
  fi
  chown -R root:sdkwork "$config_root"
  chown -R sdkwork:sdkwork "$data_root" "$log_root" "$cache_root" "$run_root"
  if [ "$start_service" = true ]; then
    systemctl daemon-reload
    systemctl enable --now sdkwork-api-gateway.service
  fi
fi

echo "installed sdkwork-api-cloud-gateway ($deployment_profile.$environment) under ${destdir:-/}"

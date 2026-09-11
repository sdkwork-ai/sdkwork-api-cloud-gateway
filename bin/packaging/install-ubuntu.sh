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
knowledgebase_rpc_binary_source=$package_root/bin/sdkwork-intelligence-knowledgebase-rpc-bin
knowledgebase_rpc_entrypoint_source=$script_dir/knowledgebase-rpc-entrypoint
unit_source=$script_dir/sdkwork-api-gateway.service
knowledgebase_rpc_unit_source=$script_dir/sdkwork-knowledgebase-rpc.service
config_source=$package_root/config/sdkwork-api-cloud-gateway.$environment.toml

for required in "$binary_source" "$knowledgebase_rpc_binary_source" \
  "$knowledgebase_rpc_entrypoint_source" "$unit_source" \
  "$knowledgebase_rpc_unit_source" "$config_source" \
  "$package_root/database-modules" "$package_root/sdkwork.app.config.json" \
  "$package_root/specs/component.spec.json"; do
  if [ ! -e "$required" ]; then
    echo "package is missing required install asset: $required" >&2
    exit 1
  fi
done

if ! "$binary_source" --version >/dev/null; then
  echo "gateway binary is not executable on this Ubuntu host" >&2
  exit 1
fi

if [ -z "$destdir" ]; then
  if ! getent group sdkwork >/dev/null 2>&1; then # PORTABILITY:target-linux
    groupadd --system sdkwork
  fi
  if ! id sdkwork >/dev/null 2>&1; then
    useradd --system --gid sdkwork --home-dir /opt/sdkwork/api-gateway sdkwork
  fi
fi

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
install -m 0755 "$knowledgebase_rpc_binary_source" \
  "$install_root/bin/sdkwork-intelligence-knowledgebase-rpc-bin"
install -m 0755 "$knowledgebase_rpc_entrypoint_source" \
  "$install_root/bin/knowledgebase-rpc-entrypoint"
install -m 0644 "$package_root/sdkwork.app.config.json" "$install_root/sdkwork.app.config.json"
install -m 0644 "$package_root/specs/component.spec.json" "$install_root/specs/component.spec.json"
cp -R "$package_root/database-modules/." "$install_root/database-modules/"
if [ -d "$package_root/data" ]; then
  cp -R "$package_root/data/." "$install_root/data/"
fi

# APPLICATION_DEPLOY_LAYOUT_SPEC: primary runtime file is config.toml.
# Legacy sdkwork-api-cloud-gateway.toml is accepted only as a migration read
# fallback by the process loader; installers must not write it as primary.
secrets_root=$config_root/secrets
install -d -m 0750 "$secrets_root"

config_target=$config_root/config.toml
if [ ! -e "$config_target" ]; then
  install -m 0640 "$config_source" "$config_target"
fi

env_target=$config_root/api-gateway.env
if [ ! -e "$env_target" ]; then
  umask 027
  {
    echo "SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE=$deployment_profile"
    echo "SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT=$environment"
    echo "SDKWORK_ENVIRONMENT=$environment"
    echo "SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID=$deployment_profile.$environment"
    echo "SDKWORK_API_CLOUD_GATEWAY_RUNTIME_TARGET=server"
    echo "SDKWORK_API_GATEWAY_CONFIG_FILE=/etc/sdkwork/api-gateway/config.toml"
    echo "SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE=/etc/sdkwork/api-gateway/config.toml"
    echo "SDKWORK_APP_ROOT=/opt/sdkwork/api-gateway"
    echo "SDKWORK_DATABASE_MODULES_ROOT=/opt/sdkwork/api-gateway/database-modules"
    echo "SDKWORK_MODELS_APP_ROOT=/opt/sdkwork/api-gateway/database-modules/sdkwork-models"
    echo "SDKWORK_MODELS_CATALOG_ROOT=/opt/sdkwork/api-gateway/data/sdkwork-models"
    echo "SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC=false"
    echo "SDKWORK_IM_PRINCIPAL_DIRECTORY=postgres"
    echo "SDKWORK_IAM_SIGNING_MASTER_SECRET_FILE=/etc/sdkwork/api-gateway/secrets/iam-signing-master.key"
    echo "SDKWORK_PAYMENT_CREDENTIAL_MASTER_KEY_FILE=/etc/sdkwork/api-gateway/secrets/payment-credential-master.key"
    echo "SDKWORK_IM_GROUP_KNOWLEDGEBASE_LAUNCH_TICKET_SECRET_FILE=/etc/sdkwork/api-gateway/secrets/knowledgebase-rpc/im-launch-ticket.key"
    echo "SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_ENDPOINT=https://127.0.0.1:50054"
    echo "SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CA_CERT_PATH=/etc/sdkwork/api-gateway/secrets/knowledgebase-rpc/ca.crt"
    echo "SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CERT_PATH=/etc/sdkwork/api-gateway/secrets/knowledgebase-rpc/client.crt"
    echo "SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_KEY_PATH=/etc/sdkwork/api-gateway/secrets/knowledgebase-rpc/client.key"
    echo "SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_TLS_DOMAIN=knowledgebase.local"
    echo "SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CALLER_CONTEXT_SIGNING_KEY_FILE=/etc/sdkwork/api-gateway/secrets/knowledgebase-rpc/caller-context.key"
    echo "SDKWORK_KNOWLEDGEBASE_SECRETS_ENCRYPTION_KEY_FILE=/etc/sdkwork/api-gateway/secrets/knowledgebase-rpc/knowledgebase-secrets.key"
    echo "SDKWORK_KNOWLEDGEBASE_ENVIRONMENT=$environment"
    echo "SDKWORK_KNOWLEDGEBASE_DEPLOYMENT_PROFILE=$deployment_profile"
    echo "SDKWORK_KNOWLEDGEBASE_TENANT_ID=100001"
    echo "SDKWORK_SECRETS_DIR=/etc/sdkwork/api-gateway/secrets"
  } > "$env_target"
  chmod 0640 "$env_target"
fi

knowledgebase_rpc_env_target=$config_root/sdkwork-knowledgebase-rpc.env
if [ ! -e "$knowledgebase_rpc_env_target" ]; then
  umask 027
  {
    echo "SDKWORK_KNOWLEDGEBASE_ENVIRONMENT=$environment"
    echo "SDKWORK_KNOWLEDGEBASE_DEPLOYMENT_PROFILE=$deployment_profile"
    echo "SDKWORK_KNOWLEDGEBASE_RPC_ENABLED=true"
    echo "SDKWORK_KNOWLEDGEBASE_RPC_BIND_ADDR=127.0.0.1:50054"
    echo "SDKWORK_KNOWLEDGEBASE_RPC_TLS_ENABLED=true"
    echo "SDKWORK_KNOWLEDGEBASE_RPC_MTLS_ENABLED=true"
    echo "SDKWORK_KNOWLEDGEBASE_RPC_HEALTH_ENABLED=true"
    echo "SDKWORK_KNOWLEDGEBASE_RPC_SERVER_CERT_PATH=/etc/sdkwork/api-gateway/secrets/knowledgebase-rpc/server.crt"
    echo "SDKWORK_KNOWLEDGEBASE_RPC_SERVER_KEY_PATH=/etc/sdkwork/api-gateway/secrets/knowledgebase-rpc/server.key"
    echo "SDKWORK_KNOWLEDGEBASE_RPC_CLIENT_CA_CERTIFICATE_PATH=/etc/sdkwork/api-gateway/secrets/knowledgebase-rpc/ca.crt"
    echo "SDKWORK_KNOWLEDGEBASE_RPC_SPIFFE_TRUST_DOMAIN=sdkwork.internal"
    echo "SDKWORK_KNOWLEDGEBASE_RPC_IM_CALLER_CONTEXT_SIGNING_KEY_FILE=/etc/sdkwork/api-gateway/secrets/knowledgebase-rpc/caller-context.key"
    if [ "$deployment_profile" = standalone ]; then
      echo "SDKWORK_KNOWLEDGEBASE_DRIVE_STORAGE_ROOT=/var/lib/sdkwork/api-gateway/knowledgebase-drive"
    else
      echo "# SDKWORK_KNOWLEDGEBASE_DRIVE_STORAGE_PROVIDER_ID=<managed-provider-id>"
    fi
    echo "SDKWORK_KNOWLEDGEBASE_OPERATOR_ID=sdkwork-knowledgebase"
    echo "SDKWORK_KNOWLEDGEBASE_ACTOR_ID=1"
    echo "SDKWORK_KNOWLEDGEBASE_TENANT_ID=100001"
    echo "SDKWORK_DATABASE_URL_FILE=/etc/sdkwork/database/database.url.secret"
    echo "SDKWORK_API_CLOUD_GATEWAY_PROVISION_KNOWLEDGEBASE_RPC_SECRETS=false"
  } > "$knowledgebase_rpc_env_target"
  chmod 0640 "$knowledgebase_rpc_env_target"
fi

install -m 0644 "$unit_source" "$unit_root/sdkwork-api-gateway.service"
install -m 0644 "$knowledgebase_rpc_unit_source" \
  "$unit_root/sdkwork-knowledgebase-rpc.service"

if [ -z "$destdir" ]; then
  SDKWORK_KNOWLEDGEBASE_RPC_SECRETS_ROOT=$secrets_root/knowledgebase-rpc \
    SDKWORK_API_CLOUD_GATEWAY_PROVISION_KNOWLEDGEBASE_RPC_SECRETS=true \
    "$install_root/bin/knowledgebase-rpc-entrypoint" --provision-only
  umask 077
  for secret_file in payment-credential-master.key iam-signing-master.key; do
    if [ ! -s "$secrets_root/$secret_file" ]; then
      openssl rand -base64 48 > "$secrets_root/$secret_file"
    fi
  done
  chown -R root:sdkwork "$config_root"
  chown root:root "$secrets_root/knowledgebase-rpc/ca.key"
  chmod 0750 "$config_root" "$secrets_root" "$secrets_root/knowledgebase-rpc"
  chmod 0640 "$config_root"/*.env
  chmod 0640 "$secrets_root"/*.key
  chmod 0640 "$secrets_root/knowledgebase-rpc"/*.key
  chmod 0644 "$secrets_root/knowledgebase-rpc"/*.crt
  chmod 0600 "$secrets_root/knowledgebase-rpc/ca.key"
  chown -R sdkwork:sdkwork "$data_root" "$log_root" "$cache_root" "$run_root"
  if [ "$start_service" = true ]; then
    if [ ! -s /etc/sdkwork/database/database.url.secret ]; then
      echo "missing /etc/sdkwork/database/database.url.secret; create the root:sdkwork 0640 PostgreSQL URL secret or rerun with --no-start" >&2
      exit 1
    fi
    systemctl daemon-reload
    systemctl enable --now sdkwork-knowledgebase-rpc.service sdkwork-api-gateway.service
  fi
fi

echo "installed sdkwork-api-cloud-gateway ($deployment_profile.$environment) under ${destdir:-/}"

#!/bin/sh
set -eu

environment=production
deployment_profile=standalone
destdir=
start_service=true
bind_value=
bind_host=
bind_port=
database_config_dir=
database_host=
database_port=
database_name=
database_username=
database_password=
database_password_file=
database_ssl_mode=
redis_url=
rate_limit_backend=
redis_key_prefix=

usage() {
  cat <<'EOF'
Usage: install-ubuntu.sh [options]

Options:
  --environment <development|test|staging|production>
  --deployment-profile <standalone|cloud>
  --destdir <path>   Stage under a package root without touching the host.
  --no-start         Install files without enabling or starting systemd.
  -h, --help         Show this help.

Installation-time runtime overrides (written to the service EnvironmentFile or
the PostgreSQL profile directory, never into the shipped config template —
PACKAGING_SPEC §5.5):

  --bind <host:port>            HTTP listener bind, for example 0.0.0.0:3901.
  --host <host>                 Listener host only; keeps the template port.
  --port <port>                 Listener port only; keeps the template host.
  --database-config-dir <path>  PostgreSQL profile directory holding
                                database.toml (or database.env) and
                                database.secret. Default /etc/sdkwork/database.
  --database-host <host>        PostgreSQL host; implies a generated
                                database.env profile.
  --database-port <port>        PostgreSQL port (default 5432).
  --database-name <database>    PostgreSQL database (and schema) name.
  --database-username <user>    PostgreSQL user.
  --database-password <value>   PostgreSQL password, written to
                                <database-config-dir>/database.secret with
                                0640 root:sdkwork and referenced through
                                SDKWORK_DATABASE_PASSWORD_FILE. Prefer
                                pre-creating the secret and using
                                --database-password-file instead of a
                                process-visible argument.
  --database-password-file <path>
                                Existing password file; referenced through
                                SDKWORK_DATABASE_PASSWORD_FILE.
  --database-ssl-mode <mode>    disable|allow|prefer|require|verify-ca|
                                verify-full (default disable).
  --redis-url <url>             Redis URL for the Web Framework shared stores.
                                Required for production-like environments.
  --rate-limit-backend <memory|redis>
                                Rate-limit backend; redis additionally needs
                                --redis-url.
  --redis-key-prefix <prefix>   Key namespace for Redis rate-limit buckets.
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
    --bind)
      require_value "$@"
      bind_value=$2
      shift 2
      ;;
    --host)
      require_value "$@"
      bind_host=$2
      shift 2
      ;;
    --port)
      require_value "$@"
      bind_port=$2
      shift 2
      ;;
    --database-config-dir)
      require_value "$@"
      database_config_dir=${2%/}
      shift 2
      ;;
    --database-host)
      require_value "$@"
      database_host=$2
      shift 2
      ;;
    --database-port)
      require_value "$@"
      database_port=$2
      shift 2
      ;;
    --database-name)
      require_value "$@"
      database_name=$2
      shift 2
      ;;
    --database-username)
      require_value "$@"
      database_username=$2
      shift 2
      ;;
    --database-password)
      require_value "$@"
      database_password=$2
      shift 2
      ;;
    --database-password-file)
      require_value "$@"
      database_password_file=$2
      shift 2
      ;;
    --database-ssl-mode)
      require_value "$@"
      database_ssl_mode=$2
      shift 2
      ;;
    --redis-url)
      require_value "$@"
      redis_url=$2
      shift 2
      ;;
    --rate-limit-backend)
      require_value "$@"
      rate_limit_backend=$2
      shift 2
      ;;
    --redis-key-prefix)
      require_value "$@"
      redis_key_prefix=$2
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

if [ -n "$bind_port" ]; then
  case "$bind_port" in
    ''|*[!0-9]*) echo "--port must be numeric: $bind_port" >&2; exit 2 ;;
  esac
  if [ "$bind_port" -lt 1 ] || [ "$bind_port" -gt 65535 ]; then
    echo "--port must be in 1..65535: $bind_port" >&2
    exit 2
  fi
fi
if [ -n "$bind_value" ]; then
  case "$bind_value" in
    *:*) ;;
    *) echo "--bind must be <host:port>: $bind_value" >&2; exit 2 ;;
  esac
fi
if [ -n "$redis_url" ]; then
  case "$redis_url" in
    redis://*|rediss://*) ;;
    *) echo "--redis-url must start with redis:// or rediss://: $redis_url" >&2; exit 2 ;;
  esac
fi
if [ -n "$rate_limit_backend" ]; then
  case "$rate_limit_backend" in
    memory|redis) ;;
    *) echo "unsupported --rate-limit-backend: $rate_limit_backend" >&2; exit 2 ;;
  esac
fi
if [ "$rate_limit_backend" = redis ] && [ -z "$redis_url" ]; then
  echo "--rate-limit-backend redis requires --redis-url" >&2
  exit 2
fi
if [ -n "$database_config_dir" ]; then
  case "$database_config_dir" in
    /*) ;;
    *) echo "--database-config-dir must be an absolute path" >&2; exit 2 ;;
  esac
fi

database_profile_requested=false
if [ -n "$database_host" ] || [ -n "$database_port" ] || [ -n "$database_name" ] \
  || [ -n "$database_username" ] || [ -n "$database_password" ] \
  || [ -n "$database_password_file" ] || [ -n "$database_ssl_mode" ]; then
  database_profile_requested=true
fi
if [ "$database_profile_requested" = true ]; then
  if [ -z "$database_name" ] || [ -z "$database_username" ]; then
    echo "--database-name and --database-username are required when generating a PostgreSQL profile" >&2
    exit 2
  fi
  if [ -n "$database_password" ] && [ -n "$database_password_file" ]; then
    echo "use either --database-password or --database-password-file, not both" >&2
    exit 2
  fi
  if [ -n "$database_password_file" ]; then
    case "$database_password_file" in
      /*) ;;
      *) echo "--database-password-file must be an absolute path" >&2; exit 2 ;;
    esac
    if [ -z "$destdir" ] && [ ! -s "$database_password_file" ]; then
      echo "--database-password-file does not exist or is empty: $database_password_file" >&2
      exit 2
    fi
  fi
fi
if [ -n "$database_port" ]; then
  case "$database_port" in
    ''|*[!0-9]*) echo "--database-port must be numeric: $database_port" >&2; exit 2 ;;
  esac
  if [ "$database_port" -lt 1 ] || [ "$database_port" -gt 65535 ]; then
    echo "--database-port must be in 1..65535: $database_port" >&2
    exit 2
  fi
fi
if [ -n "$database_ssl_mode" ]; then
  case "$database_ssl_mode" in
    disable|allow|prefer|require|verify-ca|verify-full) ;;
    *) echo "unsupported --database-ssl-mode: $database_ssl_mode" >&2; exit 2 ;;
  esac
fi

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

# Installation-time listener override: `--bind` wins; `--host`/`--port` compose
# against the shipped template so an operator only states what changes.
template_bind=$(sed -n 's/^[[:space:]]*bind[[:space:]]*=[[:space:]]*"\(.*\)"[[:space:]]*$/\1/p' "$config_source" 2>/dev/null | head -n 1)
if [ -z "$bind_value" ] && { [ -n "$bind_host" ] || [ -n "$bind_port" ]; }; then
  template_host=${template_bind%:*}
  template_port=${template_bind##*:}
  [ -n "$template_host" ] || template_host=0.0.0.0
  [ -n "$template_port" ] || template_port=3900
  if [ -n "$bind_host" ]; then
    effective_host=$bind_host
  else
    effective_host=$template_host
  fi
  if [ -n "$bind_port" ]; then
    effective_port=$bind_port
  else
    effective_port=$template_port
  fi
  bind_value=$effective_host:$effective_port
fi

# Upsert a key in the service EnvironmentFile without disturbing operator edits
# to other keys. Keeps the existing inode so ownership and 0640 mode survive.
set_env() {
  set_env_key=$1
  set_env_value=$2
  set_env_file=$3
  set_env_tmp=$set_env_file.tmp
  SET_ENV_KEY=$set_env_key SET_ENV_VALUE=$set_env_value awk '
    BEGIN { done = 0 }
    index($0, ENVIRON["SET_ENV_KEY"] "=") == 1 {
      if (!done) { print ENVIRON["SET_ENV_KEY"] "=" ENVIRON["SET_ENV_VALUE"]; done = 1 }
      next
    }
    { print }
    END { if (!done) print ENVIRON["SET_ENV_KEY"] "=" ENVIRON["SET_ENV_VALUE"] }
  ' "$set_env_file" > "$set_env_tmp"
  cat "$set_env_tmp" > "$set_env_file"
  rm -f "$set_env_tmp"
}

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
    echo "SDKWORK_DATABASE_CONFIG_DIR=${database_config_dir:-/etc/sdkwork/database}"
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

# Installation-time overrides (PACKAGING_SPEC §5.5): the EnvironmentFile is the
# only place installation-generated values land, so config.toml stays the
# shipped, reviewable template and re-running the installer is idempotent.
if [ -n "$bind_value" ]; then
  set_env SDKWORK_API_CLOUD_GATEWAY_BIND "$bind_value" "$env_target"
fi
if [ -n "$database_config_dir" ]; then
  set_env SDKWORK_DATABASE_CONFIG_DIR "$database_config_dir" "$env_target"
fi
if [ -n "$redis_url" ]; then
  set_env SDKWORK_API_CLOUD_GATEWAY_WEB_REDIS_URL "$redis_url" "$env_target"
  set_env SDKWORK_API_CLOUD_GATEWAY_RATE_LIMIT_REDIS_URL "$redis_url" "$env_target"
fi
if [ -n "$rate_limit_backend" ]; then
  set_env SDKWORK_API_CLOUD_GATEWAY_RATE_LIMIT_BACKEND "$rate_limit_backend" "$env_target"
fi
if [ -n "$redis_key_prefix" ]; then
  set_env SDKWORK_API_CLOUD_GATEWAY_RATE_LIMIT_REDIS_KEY_PREFIX "$redis_key_prefix" "$env_target"
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

# Keep the bundled knowledgebase RPC sidecar on the same PostgreSQL profile the
# gateway was installed with.
if [ -n "$database_config_dir" ]; then
  set_env SDKWORK_DATABASE_CONFIG_DIR "$database_config_dir" "$knowledgebase_rpc_env_target"
fi

# Installation-time PostgreSQL profile: --database-* connection options write
# the canonical database.env profile. The password stays in a root:sdkwork 0640
# secret file referenced through SDKWORK_DATABASE_PASSWORD_FILE, so credentials
# never enter an EnvironmentFile, a systemd unit, or the config template
# (PACKAGING_SPEC §5.5).
if [ "$database_profile_requested" = true ]; then
  database_profile_dir=${database_config_dir:-/etc/sdkwork/database}
  # `--destdir` stages a filesystem image: the profile lands under the stage
  # root while every generated reference keeps the runtime path the deployed
  # host will read.
  database_profile_stage_dir=$destdir$database_profile_dir
  if [ -f "$database_profile_stage_dir/database.toml" ]; then
    echo "$database_profile_dir/database.toml already defines the PostgreSQL profile; edit it directly or pass a different --database-config-dir" >&2
    exit 1
  fi
  database_host=${database_host:-127.0.0.1}
  database_port=${database_port:-5432}
  database_ssl_mode=${database_ssl_mode:-disable}
  install -d -m 0750 "$database_profile_stage_dir"
  if [ -n "$database_password" ]; then
    umask 027
    printf '%s\n' "$database_password" > "$database_profile_stage_dir/database.secret"
    database_password_file=$database_profile_dir/database.secret
  fi
  umask 027
  {
    echo "SDKWORK_DATABASE_ENGINE=postgresql"
    echo "SDKWORK_DATABASE_HOST=$database_host"
    echo "SDKWORK_DATABASE_PORT=$database_port"
    echo "SDKWORK_DATABASE_NAME=$database_name"
    echo "SDKWORK_DATABASE_SCHEMA=$database_name"
    echo "SDKWORK_DATABASE_USERNAME=$database_username"
    [ -z "$database_password_file" ] || echo "SDKWORK_DATABASE_PASSWORD_FILE=$database_password_file"
    echo "SDKWORK_DATABASE_SSL_MODE=$database_ssl_mode"
  } > "$database_profile_stage_dir/database.env"
  chmod 0640 "$database_profile_stage_dir/database.env"
  if [ -n "$database_password" ]; then
    chmod 0640 "$database_profile_stage_dir/database.secret"
  fi
  if [ -z "$destdir" ]; then
    chown root:sdkwork "$database_profile_stage_dir/database.env"
    if [ -n "$database_password" ]; then
      chown root:sdkwork "$database_profile_stage_dir/database.secret"
    fi
  fi
  set_env SDKWORK_DATABASE_CONFIG_DIR "$database_profile_dir" "$env_target"
  set_env SDKWORK_DATABASE_CONFIG_DIR "$database_profile_dir" "$knowledgebase_rpc_env_target"
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
    # Canonical PostgreSQL profile (DATABASE_SPEC / sdkwork-database-config):
    # `database.toml` (or `database.env`) in the workspace config dir, with
    # secret material referenced by `password_file`. The legacy standalone
    # `database.url.secret` stays accepted for pre-migration hosts.
    database_profile_dir=${database_config_dir:-/etc/sdkwork/database}
    if [ ! -f "$database_profile_dir/database.toml" ] \
      && [ ! -f "$database_profile_dir/database.env" ] \
      && [ ! -s "$database_profile_dir/database.url.secret" ]; then
      echo "missing PostgreSQL profile in $database_profile_dir: expected database.toml (or database.env) with password_file pointing at a root:sdkwork 0640 secret, or the legacy database.url.secret; pass --database-config-dir to point elsewhere, or rerun with --no-start" >&2
      exit 1
    fi
    # Production-like environments compose the Web Framework shared stores on
    # Redis; without a URL the gateway refuses to start, so fail at install time
    # instead of leaving a crash-looping unit behind.
    case "$environment" in
      prod|production|stage|staging|live)
        if ! grep -q '^SDKWORK_API_CLOUD_GATEWAY_WEB_REDIS_URL=..*' "$env_target" 2>/dev/null; then
          echo "production-like installs require SDKWORK_API_CLOUD_GATEWAY_WEB_REDIS_URL for the Web Framework shared stores; rerun with --redis-url <url>, or rerun with --no-start" >&2
          exit 1
        fi
        ;;
    esac
    systemctl daemon-reload
    systemctl enable --now sdkwork-knowledgebase-rpc.service sdkwork-api-gateway.service
  fi
fi

echo "installed sdkwork-api-cloud-gateway ($deployment_profile.$environment) under ${destdir:-/}"

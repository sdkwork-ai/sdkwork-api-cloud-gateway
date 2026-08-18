#!/bin/bash
# Deploy sdkwork-api-cloud-gateway in external-dependency Docker mode.
#
# Each environment runs as an isolated compose project with a distinct host
# port (3910-3913) and sdkwork-specs database identity (sdkwork_ai_dev/test/
# staging/prod). External PostgreSQL and Redis must already be reachable from
# the container (typically host.docker.internal on WSL).
#
# Usage:
#   scripts/deploy-docker-environment.sh development
#   scripts/deploy-docker-environment.sh all
#   scripts/deploy-docker-environment.sh test --pull
set -eu

usage() {
  cat <<'EOF'
Usage: deploy-docker-environment.sh <environment|all> [options]

Environments: development, test, staging, production, all

Options:
  --pull          docker compose pull before up
  --down          stop the selected stack instead of starting it
  --validate      run pnpm check:container-deployment before compose up
  -h, --help      show this help
EOF
}

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
pull=false
down=false
validate=false
target=${1:-}

shift || true
while [ "$#" -gt 0 ]; do
  case "$1" in
    --pull) pull=true; shift ;;
    --down) down=true; shift ;;
    --validate) validate=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unsupported option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ -z "$target" ]; then
  usage >&2
  exit 2
fi

deploy_one() {
  env_name=$1
  env_file="$repo_root/docker/env/$env_name.env"
  project="sdkwork-gateway-$env_name"

  if [ ! -f "$env_file" ]; then
    echo "missing env file: $env_file" >&2
    echo "copy docker/env/$env_name.env.example and fill secrets first" >&2
    exit 1
  fi

  if [ "$validate" = true ] && [ "$down" = false ]; then
    (cd "$repo_root" && pnpm check:container-deployment -- --env-file "$env_file" --mode external --compose)
  fi

  compose_args=(
    --env-file "$env_file"
    -p "$project"
    -f docker-compose.yml
    -f docker-compose.external.yml
  )

  if [ "$down" = true ]; then
    docker compose "${compose_args[@]}" down
    echo "stopped $env_name ($project)"
    return 0
  fi

  if [ "$pull" = true ]; then
    docker compose "${compose_args[@]}" pull
  fi

  docker compose "${compose_args[@]}" up -d
  port=$(grep -E '^GATEWAY_HOST_PORT=' "$env_file" | cut -d= -f2-)
  echo "deployed $env_name ($project) -> http://127.0.0.1:${port:-?}/healthz"
}

case "$target" in
  development|test|staging|production)
    deploy_one "$target"
    ;;
  all)
    for env_name in development test staging production; do
      deploy_one "$env_name"
    done
    ;;
  *)
    echo "unsupported environment: $target" >&2
    usage >&2
    exit 2
    ;;
esac

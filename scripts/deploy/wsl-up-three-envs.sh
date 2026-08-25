#!/bin/bash
set -euo pipefail
cd "$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)"
mkdir -p docker/ca
[[ -f docker/ca/postgres-ca-bundle.crt ]] || : > docker/ca/postgres-ca-bundle.crt

COMPAT=(-f docker-compose.yml -f docker-compose.image-path-compat.yml)

for name in development test production; do
  echo "==> deploy $name"
  docker compose --env-file "docker/env/${name}.env" -p "sdkwork-gateway-${name}" \
    "${COMPAT[@]}" up -d
done

echo "==> waiting for readiness"
for i in 1 2 3 4 5 6 7 8 9 10 11 12; do
  ok=0
  for port in 3910 3911 3913; do
    if curl -fsS --max-time 2 "http://127.0.0.1:${port}/healthz" >/dev/null 2>&1; then
      ok=$((ok+1))
    fi
  done
  echo "  round $i: $ok/3 healthy"
  [[ "$ok" -eq 3 ]] && break
  sleep 10
done

echo "==> status"
docker ps --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}' | grep -E 'sdkwork-gateway-(development|test|production)|NAMES' || true

echo "==> health probes"
for port in 3910 3911 3913; do
  printf '  :%s -> ' "$port"
  curl -fsS --max-time 5 "http://127.0.0.1:${port}/healthz" || echo FAIL
  echo
done

# Show recent errors if any still failing
for name in development test production; do
  cid=$(docker ps -aq -f "name=sdkwork-gateway-${name}-gateway-1" | head -1)
  if [[ -n "$cid" ]]; then
    st=$(docker inspect -f '{{.State.Status}} {{.State.ExitCode}}' "$cid")
    echo "==> $name gateway state: $st"
    if [[ "$st" != healthy* ]] && [[ "$st" != *running* ]]; then
      docker logs --tail 20 "$cid" 2>&1 | tail -20 || true
    fi
  fi
done

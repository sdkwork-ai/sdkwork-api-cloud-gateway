# Docker deployment assets

Multi-environment standalone container deployment for WSL Ubuntu / Linux hosts.

## Layout parity with Ubuntu install

`topology.applicationCode` = `api_gateway` (APPLICATION_DEPLOY_LAYOUT_SPEC).
Container mounts use the **same** FHS paths as `scripts/install-ubuntu.sh`:

| Purpose | Path |
| --- | --- |
| Config | `/etc/sdkwork/api_gateway/config.toml` |
| Process env (host install) | `/etc/sdkwork/api_gateway/api_gateway.env` |
| Secrets (container) | `/run/secrets/sdkwork/api_gateway` |
| Secrets (host install) | `/etc/sdkwork/api_gateway/secrets/` |
| Data | `/var/lib/sdkwork/api_gateway` |
| Install root | `/opt/sdkwork/api_gateway` |

Host-side override for containers: `docker/config/config.toml` →
`/etc/sdkwork/api_gateway/config.toml`.

## Boundary: sdkwork-webserver owns public reverse proxy

`sdkwork-webserver` is an **independent** Docker deployment. This gateway
composition:

- does **not** start sdkwork-webserver
- does **not** install nginx site mappings
- exposes only `GATEWAY_HOST_PORT` → container `:3900` for health and for
  sdkwork-webserver to reverse-proxy

## Default dependencies: external PostgreSQL + Redis (no password)

Per `RUNTIME_DIRECTORY_SPEC` §4.5, production-like containers use external
PostgreSQL and Redis. Defaults:

- PostgreSQL: `host.docker.internal:5432`
- Redis: `host.docker.internal:6380`, **empty password**

Optional in-compose databases:

```bash
docker compose --profile embedded \
  -f docker-compose.yml -f docker-compose.embedded.yml up -d
```

## Quick start

```bash
# 1. Provision databases (sdkwork_ai_dev/test/staging/prod)
pnpm db:postgres:provision

# 2. Configure secrets in docker/env/<environment>.env (from *.env.example)

# 3. Build image and deploy (external PG/Redis; no nginx)
pnpm build:container
pnpm deploy:apply:standalone -- development --validate
# or: pnpm deploy:apply -- --env development test production
```

## Port and domain matrix

| Environment | Host port | Database | Domains (proxied by sdkwork-webserver) |
| --- | --- | --- | --- |
| development | 3910 | `sdkwork_ai_dev` | `api-dev.{sdkwork,birdcoder,dtupay}.com` |
| test | 3911 | `sdkwork_ai_test` | `api-test.{sdkwork,birdcoder,dtupay}.com` |
| staging | 3912 | `sdkwork_ai_staging` | `api-staging.{sdkwork,birdcoder,dtupay}.com` |
| production | 3913 | `sdkwork_ai_prod` | `api.{sdkwork,birdcoder,dtupay}.com` |

## Commands

| Public script | Implementation | Purpose |
| --- | --- | --- |
| `pnpm deploy:apply:standalone -- <env\|all>` | `scripts/deploy-docker-environment.sh` | Start/stop standalone environments (external mode) |
| `pnpm db:postgres:provision` | `scripts/provision-external-databases.sh` | Create sdkwork-specs workspace databases |
| `pnpm deploy:apply` / `pnpm deploy:apply:wsl` | `scripts/deploy/apply.sh` (+ `apply.ps1`) | Build + deploy all envs + health check (no nginx) |
| `pnpm deploy:host:prepare` | `scripts/deploy/prepare-host.sh` | Install/start host PostgreSQL + Redis |
| `pnpm install:hosts` | `scripts/bind-windows-hosts.ps1` | Bind Windows hosts entries |

Full operator guide: [`docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md`](../docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md).

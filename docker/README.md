# Docker deployment assets

Multi-environment standalone container deployment for WSL Ubuntu / Linux hosts.

## Layout parity with Ubuntu install

`topology.applicationCode` = `api-gateway` (APPLICATION_DEPLOY_LAYOUT_SPEC).
Container mounts use the **same** FHS paths as `scripts/install-ubuntu.sh`:

| Purpose | Path |
| --- | --- |
| Config | `/etc/sdkwork/api-gateway/config.toml` |
| Process env (host install) | `/etc/sdkwork/api-gateway/api-gateway.env` |
| Secrets (container) | `/run/secrets/sdkwork/api-gateway` |
| Secrets (host install) | `/etc/sdkwork/api-gateway/secrets/` |
| Data | `/var/lib/sdkwork/api-gateway` |
| Install root | `/opt/sdkwork/api-gateway` |

Host-side override for containers: `docker/config/config.toml` →
`/etc/sdkwork/api-gateway/config.toml`.

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

# Unified-bundle path (one image, any environment, N instances per env):
pnpm build:container:install
pnpm deploy:apply:standalone:docker -- --environment production --replicas 3
```

## Port and domain matrix

Each environment supports N gateway instances. Instance `i` publishes
`GATEWAY_HOST_PORT + (i - 1) * 10` -> container `:3900` (deterministic,
collision-free across environments). See
[`docs/guides/operator/docker-install.md`](../docs/guides/operator/docker-install.md).

| Environment | Base host port (instance 1) | Database | Domains (proxied by sdkwork-webserver) |
| --- | --- | --- | --- |
| development | 3910 | `sdkwork_ai_dev` | `api-dev.{sdkwork,birdcoder,dtupay}.com` |
| test | 3911 | `sdkwork_ai_test` | `api-test.{sdkwork,birdcoder,dtupay}.com` |
| staging | 3912 | `sdkwork_ai_staging` | `api-staging.{sdkwork,birdcoder,dtupay}.com` |
| production | 3913 | `sdkwork_ai_prod` | `api.{sdkwork,birdcoder,dtupay}.com` |

## Commands

| Public script | Implementation | Purpose |
| --- | --- | --- |
| `pnpm build:container` | `scripts/build-api-cloud-gateway-container.mjs` | Build the unified environment-neutral image |
| `pnpm build:container:install` | `scripts/docker/package-install-bundle.mjs` | Build image + package the self-contained install bundle (`dist/docker-install/*.bundle/`) |
| `pnpm deploy:apply:standalone:docker -- --environment <env> [--replicas N]` | `scripts/docker/bundle/deploy.sh` | Deploy the unified bundle: any environment, N instances per environment |
| `pnpm deploy:apply:standalone:docker:remote -- --host <user@host> --environment <env> [--replicas N]` | `scripts/docker/remote-deploy.sh` | Package locally, push to a remote Ubuntu host over SSH key auth, deploy with env config centralized at `/etc/sdkwork/api-gateway/docker/<env>.env` |
| `pnpm deploy:apply:standalone -- <env\|all>` | `scripts/deploy-docker-environment.sh` | Start/stop per-environment stacks from the repository compose (external mode) |
| `pnpm db:postgres:provision` | `scripts/provision-external-databases.sh` | Create sdkwork-specs workspace databases |
| `pnpm deploy:apply` / `pnpm deploy:apply:wsl` | `scripts/deploy/apply.sh` (+ `apply.ps1`) | Build + deploy all envs + health check (no nginx) |
| `pnpm deploy:host:prepare` | `scripts/deploy/prepare-host.sh` | Install/start host PostgreSQL + Redis |
| `pnpm install:hosts` | `scripts/bind-windows-hosts.ps1` | Bind Windows hosts entries |

Space-level entrypoint (WSL / CI): `bash bin/build-gateway-docker.sh` in the
`sdkwork-space` root packages the same bundle.

Full operator guide: [`docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md`](../docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md).

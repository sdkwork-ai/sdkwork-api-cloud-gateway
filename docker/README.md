# Docker deployment assets

Multi-environment standalone container deployment for WSL Ubuntu / Linux hosts.

## Layout parity with Ubuntu install

`topology.applicationCode` = `api-gateway` (APPLICATION_DEPLOY_LAYOUT_SPEC).
Container mounts use the **same** FHS paths as `bin/packaging/install-ubuntu.sh`:

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
- Redis: `host.docker.internal:6379` (canonical host port, `DEPLOYMENT_SPEC.md` §6.1), **empty password**

Optional in-compose databases:

```bash
docker compose --profile embedded \
  -f docker-compose.yml -f docker-compose.embedded.yml up -d
```

## Quick start

The operator channel is the `bin/` nine-entrypoint family (`MODULE_BIN_SPEC.md`);
this directory holds the deployment assets those entrypoints drive.

```bash
# 1. (Optional) pre-provision host databases (sdkwork_ai_dev/test/staging/prod).
#    Install/start also runs --provision-databases, --bootstrap-databases, and
#    --migrate-databases, so a fresh host can instead supply
#    GATEWAY_POSTGRES_ADMIN_* in the env file.
pnpm db:postgres:provision

# 2. Configure secrets (docker/env/<environment>.env from *.env.example)

# 3. Build the image and deploy via bin/ (external PG/Redis; no nginx)
bin/docker-image.sh build
bin/docker-deploy.sh install --environment development
bin/docker-deploy.sh install --environment production --yes --replicas 3
bin/docker-deploy.sh status  --environment production
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
| demo | 3914 | `sdkwork_ai_demo` | `api-demo.{sdkwork,birdcoder,dtupay}.com` |
| production | 3913 | `sdkwork_ai_prod` | `api.{sdkwork,birdcoder,dtupay}.com` |

## Commands

Operator channel — `bin/` only (`MODULE_BIN_SPEC.md`; see
`docs/guides/operator/docker-install.md` and `RELEASE_LIFECYCLE.md`):

| Operator command | Purpose |
| --- | --- |
| `bin/docker-image.sh build` | Build the unified environment-neutral image |
| `bin/docker-image.sh save / load` | Offline image transport (tar.gz + sha256) |
| `bin/docker-deploy.sh install/upgrade --environment <env> [--host ...] [--replicas N]` | Deploy the bundle: any environment, N instances, health-gated |
| `bin/docker-deploy.sh rollback / status / logs / stop / start / restart / down` | Day-2 operations |
| `bin/config.sh list/show/get/set/diff/validate` | Deployed configuration (secrets redacted) |
| `bin/doctor.sh --environment <env>` | Aggregated diagnostics |
| `bin/backup.sh create/list/verify/restore` | Backup and restore |

Internal implementation (invoked by `bin/`, not an operator surface):
`scripts/build-api-cloud-gateway-container.mjs` (image build),
`scripts/docker/package-install-bundle.mjs` (bundle packaging),
`bin/docker-bundle-deploy.sh` / `bin/docker-bundle-release.sh` /
`bin/docker-bundle-prepare-envs.sh` (bundle executor sources; the packager copies
them to the bundle root as `deploy.sh` / `release.sh` / `prepare-envs.sh`),
`bin/host/prepare-host.sh` (host PostgreSQL + Redis; also issues the host CA and
writes `docker/ca/postgres-ca-bundle.crt`),
`bin/host/provision-external-databases.sh` (database provisioning, reads
`bin/host/external-schema.sql`),
`bin/host/bind-windows-hosts.ps1` (Windows hosts entries).

Space-level entrypoint (WSL / CI): `bash bin/build-gateway-docker.sh` in the
`sdkwork-space` root packages the same bundle.

Full operator guide: [`docs/guides/operator/docker-install.md`](../docs/guides/operator/docker-install.md).

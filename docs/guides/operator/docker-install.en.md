# sdkwork-api-cloud-gateway Docker Install And Deployment Guide

> Authority: `sdkwork-specs/DOCKER_SPEC.md`, `sdkwork-specs/MODULE_BIN_SPEC.md`, `sdkwork-specs/DEPLOYMENT_SPEC.md` §6/§6.1, `sdkwork-specs/APPLICATION_GATEWAY_SPEC.md`.
> This is the single authoritative Docker install document; it supersedes and absorbs `DOCKER_STANDALONE_QUICK_DEPLOY.md`, `REMOTE_DEPLOY_UBUNTU_DOCKER.md`, `INSTALLATION_STANDARDS.md`, `REMOTE_DEPLOY_BUNDLE.md/.en.md`, and `WSL_EXTERNAL_DEPLOY.md`. 中文版: [docker-install.md](./docker-install.md)

**Role boundary:** the gateway is the platform API gateway, deployed as an **independent** Docker composition that is **never exposed publicly**, installs no nginx site mappings, and never starts the webserver; the public edge is `sdkwork-webserver` reverse proxying to it (`DOCKER_SPEC.md` §1). The container publishes only the health/proxy port `GATEWAY_HOST_PORT` → container `:3900`.

---

## 0. Quick Start (TL;DR)

```bash
bin/docker-image.sh build --image-tag <version>        # → registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:<version>
bin/docker-deploy.sh install --environment demo        # local WSL (external PostgreSQL/Redis is the default)
bin/docker-deploy.sh install --environment demo --host ssh://ops@10.0.0.8   # remote Ubuntu
```

Offline install (when the build host cannot reach the target, still run bin/ from the repo side):

```bash
bin/docker-image.sh save -o dist/image.tar.gz              # produces image tar.gz + sha256
bin/docker-deploy.sh install --environment <environment> --host ssh://[user@]host [--replicas N]
```

bin/ handles bundle sync, sha256 verification, image loading and the health gate; the
in-bundle executor is invoked by bin/ — never run scripts by hand.

## 1. Prerequisites

| Item | Requirement |
| --- | --- |
| Docker Engine + compose plugin | required |
| Host PostgreSQL | `host.docker.internal:5432` (per-environment databases `sdkwork_ai_dev/test/staging/demo/prod`) |
| Host Redis | `host.docker.internal:6379` (no password; per-environment key prefix `cloudrouter-<env>`) |
| webserver | Public ingress is provided by `sdkwork-webserver` (deployed independently, before or after) |

## 2. Five-Environment Matrix

| Environment | Host port key (default) | Profile id | Domain family (ALLOWED_HOSTS / CORS) | Database |
| --- | --- | --- | --- | --- |
| development | `GATEWAY_DEV_HOST_PORT` (3910) | `standalone.development` | `api-dev.<base-domain>` full family | `sdkwork_ai_dev` |
| test | 3911 | `standalone.test` | `api-test.*` | `sdkwork_ai_test` |
| staging | 3912 | `standalone.staging` | `api-staging.*` | `sdkwork_ai_staging` |
| demo | 3914 | `standalone.demo` | `api-demo.*` | `sdkwork_ai_demo` |
| production | 3913 | `standalone.production` | `api.*` | `sdkwork_ai_prod` |

- Containers always listen on `:3900`; multi-instance port stride `GATEWAY_HOST_PORT + (index-1)*10`.
- `GATEWAY_ALLOWED_HOSTS` must enumerate the complete registered base-domain family (`ENVIRONMENT_SPEC.md` §5.1.0.1).
- Browser CORS has two halves: `GATEWAY_CORS_ALLOWED_ORIGINS` carries only the exact origins the pattern cannot express (the per-environment loopback ports), while the console closure is declared by the **registered console host pattern** — `GATEWAY_CORS_CONSOLE_HOST_LABELS` (label registry, 111 platform labels) + `GATEWAY_CORS_CONSOLE_HOST_SUFFIX` (`-dev`/`-test`/`-staging`/`-demo`/empty in production) + `GATEWAY_CORS_CONSOLE_HOST_SCHEMES` (`http,https` outside production, `https` in production). Matching stays exact per component and never degrades into an arbitrary sub-domain wildcard; `app://*`, `tauri://*` and the mini-program origin are merged by the framework and must not be written into the env file. Do not enumerate the console origins into `*_CORS_ALLOWED_ORIGINS` again (~3000 entries / 100 KiB, close to the Linux 128 KiB single-variable limit).
- Authoritative env values: `docker/env/<environment>.env` (copied from `*.env.example`, secrets filled in). After editing, run `node scripts/derive-cors-allowed-origins.mjs --apply`; `pnpm check:cors-carriers` is the gate.

## 3. Standard Install Flow (bin/ entrypoints)

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment <env> [--replicas N] [--host wsl|ssh://[user@]host[:port]]
bin/docker-deploy.sh upgrade  --environment <env> [--image-tag <new>]
bin/docker-deploy.sh rollback --environment <env>
bin/docker-deploy.sh status   --environment <env>
bin/docker-deploy.sh down     --environment <env> [--purge]   # --purge requires --yes
```

- `install` syncs this module's bundle (env/ + compose + built-in executor) to `/opt/deploy/sdkwork-api-cloud-gateway/bundle` on the target host and drives the rolling apply from there via bin/; idempotent.
- This bundle ships a release ledger, so `rollback [--to <version>]` works against it.
- Multi-instance: instance 1 completes migration and reaches health before instances 2..N start (`DOCKER_SPEC.md` §4.2).
- The knowledgebase-rpc singleton provisions shared secrets material before instance 1 via the `<project>-deps` project.
- **§4 expands every command for all five environments, copy-paste ready.**

## 4. Per-Environment Command Reference (copy-paste ready)

> Every block below can be copied and run as-is. Replace `<version>` with the
> real version (current value: `agent.app.config.json` →
> `release.currentVersion`; **omit `--image-tag` to use it automatically**),
> replace `ops@10.0.0.8` with your remote Ubuntu host, and note that `--host`
> defaults to `wsl` (local WSL) when omitted. Any mutating command accepts
> `--dry-run` to print the plan without executing it.
> This bundle ships a release ledger, so `rollback --to <version>` is effective.

### 4.1 development

Port `3910` · profile `standalone.development` · domains `api-dev.*` · database `sdkwork_ai_dev`

```bash
bin/docker-image.sh build --image-tag <version>                    # shared by all environments; skip if already built
bin/docker-deploy.sh install  --environment development            # local WSL (idempotent)
bin/docker-deploy.sh install  --environment development --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment development --image-tag <new-version>
bin/docker-deploy.sh rollback --environment development
bin/docker-deploy.sh rollback --environment development --to 0.1.0
bin/docker-deploy.sh status   --environment development
bin/docker-deploy.sh logs     --environment development
bin/docker-deploy.sh down     --environment development
bin/docker-deploy.sh stop    --environment development           # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment development           # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment development           # restart app instances only (deps and gateway stay up)
bin/docker-deploy.sh down     --environment development --purge --yes

curl --noproxy '*' http://127.0.0.1:3910/healthz
```

### 4.2 test

Port `3911` · profile `standalone.test` · domains `api-test.*` · database `sdkwork_ai_test`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment test
bin/docker-deploy.sh install  --environment test --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment test --image-tag <new-version>
bin/docker-deploy.sh rollback --environment test
bin/docker-deploy.sh rollback --environment test --to 0.1.0
bin/docker-deploy.sh status   --environment test
bin/docker-deploy.sh logs     --environment test
bin/docker-deploy.sh down     --environment test
bin/docker-deploy.sh stop    --environment test                  # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment test                  # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment test                  # restart app instances only (deps and gateway stay up)
bin/docker-deploy.sh down     --environment test --purge --yes

curl --noproxy '*' http://127.0.0.1:3911/healthz
```

### 4.3 staging

Port `3912` · profile `standalone.staging` · domains `api-staging.*` · database `sdkwork_ai_staging`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment staging
bin/docker-deploy.sh install  --environment staging --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment staging --image-tag <new-version>
bin/docker-deploy.sh rollback --environment staging
bin/docker-deploy.sh rollback --environment staging --to 0.1.0
bin/docker-deploy.sh status   --environment staging
bin/docker-deploy.sh logs     --environment staging
bin/docker-deploy.sh down     --environment staging
bin/docker-deploy.sh stop    --environment staging               # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment staging               # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment staging               # restart app instances only (deps and gateway stay up)
bin/docker-deploy.sh down     --environment staging --purge --yes

curl --noproxy '*' http://127.0.0.1:3912/healthz
```

### 4.4 demo

Port `3914` · profile `standalone.demo` · domains `api-demo.*` · database `sdkwork_ai_demo`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment demo
bin/docker-deploy.sh install  --environment demo --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment demo --image-tag <new-version>
bin/docker-deploy.sh rollback --environment demo
bin/docker-deploy.sh rollback --environment demo --to 0.1.0
bin/docker-deploy.sh status   --environment demo
bin/docker-deploy.sh logs     --environment demo
bin/docker-deploy.sh down     --environment demo
bin/docker-deploy.sh stop    --environment demo                  # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment demo                  # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment demo                  # restart app instances only (deps and gateway stay up)
bin/docker-deploy.sh down     --environment demo --purge --yes

curl --noproxy '*' http://127.0.0.1:3914/healthz
```

### 4.5 production

Port `3913` · profile `standalone.production` · domains `api.*` · database `sdkwork_ai_prod`

> **Every mutating command requires `--yes`**, otherwise it is refused
> (error code 68). `down --purge` requires `--yes` in **every** environment.

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment production --yes
bin/docker-deploy.sh install  --environment production --yes --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment production --yes --image-tag <new-version>
bin/docker-deploy.sh rollback --environment production --yes
bin/docker-deploy.sh rollback --environment production --yes --to 0.1.0
bin/docker-deploy.sh status   --environment production             # read-only, no --yes needed
bin/docker-deploy.sh logs     --environment production             # read-only, no --yes needed
bin/docker-deploy.sh down     --environment production
bin/docker-deploy.sh stop    --environment production            # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment production            # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment production            # restart app instances only (deps and gateway stay up)
bin/docker-deploy.sh down     --environment production --purge --yes

curl --noproxy '*' http://127.0.0.1:3913/healthz
```

### 4.6 Multi-instance and dependency mode

```bash
# Multi-instance: instance 1 completes migration and reaches health before 2..N start (port stride +10)
bin/docker-deploy.sh install --environment demo --replicas 3

# Dependency mode: external host PostgreSQL/Redis is the default; embedded is an explicit opt-in
bin/docker-deploy.sh install --environment demo --deps external     # default
bin/docker-deploy.sh install --environment demo --deps embedded
```

## 5. Verification

```bash
curl http://127.0.0.1:3910/healthz    # development (3911/3912/3914/3913 for the others)
# Public-plane verification must go through the webserver (Host-header probe):
# curl --noproxy '*' -H 'Host: api-demo.sdkwork.com' http://<webserver-host>:19098/healthz
node scripts/validate-docker-deployment.mjs --help
```

**Ports and full copy-paste ready commands for every environment: §4.1–§4.5.**

### 5.1 Configuration Preflight and Env-File Key Contract

```bash
# Check only, deploy nothing: fail-closed validation of every required key
bin/config.sh validate --environment production

# Show the current configuration (secrets redacted)
bin/config.sh show --environment production

# Set a config value (backs up and validates before mutating)
bin/config.sh set --environment production --key GATEWAY_POSTGRES_PASSWORD --value '<value>'

# Config diff (deployed state vs bundle baseline)
bin/config.sh diff --environment production
```

**Env-file key contract (`env/<environment>.env`)**:
- Host-side keys always use the `GATEWAY_` prefix; docker compose maps them
  to the container-side `SDKWORK_*` names. Compose only interpolates the
  variables it explicitly references — any env-file key compose never
  references is dead weight and never reaches the container.
- The **only** `SDKWORK_*` keys the compose templates consume from the env
  file are the database pool tunables (`SDKWORK_DATABASE_AUTO_MIGRATE` /
  `MAX_CONNECTIONS` / `MIN_CONNECTIONS` / `ACQUIRE_TIMEOUT` / `IDLE_TIMEOUT`
  / `MAX_LIFETIME` / `TEMPORARY_*`), the optional `SDKWORK_DATABASE_URL`
  credential override, and the install-time provisioning switch/admin
  connection (host-side `GATEWAY_DATABASE_PROVISION` /
  `GATEWAY_POSTGRES_ADMIN_*` map to container-side
  `SDKWORK_DATABASE_PROVISION` / `SDKWORK_DATABASE_ADMIN_*`). Do not put any
  other `SDKWORK_*` keys in the env file (container env comes from the image
  ENV + the compose environment section).
- Install/start runs `--provision-databases` (role, database, schema,
  extensions, grants), then `--bootstrap-databases` (module tables only), then
  waits for the Knowledgebase RPC sidecar, then runs `--migrate-databases`
  (owner assembly). A fresh host needs the admin connection; an existing
  database can be converged by the application role. Set
  `GATEWAY_DATABASE_PROVISION=false` to opt out when schema lifecycle is owned
  out of band.
- `test`/`staging`/`demo`/`production` also require
  `GATEWAY_MEMORY_CURSOR_SIGNING_KEY` (>=32 bytes) and
  `GATEWAY_MEMORY_OUTBOX_DELIVERY_MODE=http` with a resolvable HTTPS
  `GATEWAY_MEMORY_OUTBOX_DELIVERY_URL`; preflight reports every missing value.
- The foundation suite defaults to `SDKWORK_DATABASE_MAX_CONNECTIONS=150`;
  embedded PostgreSQL defaults to `PG_MAX_CONNECTIONS=400`. External PostgreSQL
  must size for the process pool plus temporary-driver pools plus admin/operator
  reserve, otherwise first assembly can fail with `pool timed out`.
- When preflight fails it collects **all** gaps in one run and prints
  foolproof fix instructions (`--set` or edit the env file directly).

## 6. Integration With The Webserver

- The webserver reverse-proxies the `api-*` platform domains to the gateway upstream (`sdkwork-api-cloud-gateway:8080` or the host port) through `imports.d/import.conf` (default `cloud` set).
- This composition configures **no** webserver/nginx content; attach mode has the webserver side join this network via `SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK`.

## 6.1 Operations Lifecycle (logs / configuration / diagnostics / backup)

```bash
# Logs: bounded read by default; --follow streams
bin/docker-deploy.sh logs --environment <env> [--instance N] [--service gateway]
bin/docker-deploy.sh logs --environment <env> --tail 1000 --since 15m
bin/docker-deploy.sh logs --environment <env> --tail 5000 --export ./incident

# Configuration: secrets redacted by default (***REDACTED***); set/edit backs up
# and validates, production requires --yes
bin/config.sh list     --environment <env>
bin/config.sh show     --environment <env>
bin/config.sh get      --environment <env> --key <KEY> [--reveal]
bin/config.sh set      --environment <env> --key <KEY> --value '<VALUE>'
bin/config.sh diff     --environment <env>      # missing / undeclared / placeholder keys
bin/config.sh validate --environment <env>
EDITOR=vi bin/config.sh edit --environment <env>

# Diagnostics: read-only, 9 checks, exit code 70 when any check fails
bin/doctor.sh --environment <env> [--json] [--export ./incident]

# Backup and restore: sets live on the target at /opt/deploy/<module>/backups/
bin/backup.sh create  --environment <env> [--no-db] [--no-volumes]
bin/backup.sh list    --environment <env>
bin/backup.sh verify  --environment <env> [--set <name>]
bin/backup.sh restore --environment <env> --set <name> --yes
```

Per-environment commands and the full action paths live in `docs/runbooks/`
(deploy / log-reference / troubleshooting / backup-restore, Chinese and
English).

## 7. Spec Index

| Topic | Authority |
| --- | --- |
| Image naming, bundle, five-environment matrix, ports | `sdkwork-specs/DOCKER_SPEC.md` |
| `bin/` entrypoint contract | `sdkwork-specs/MODULE_BIN_SPEC.md` |
| External dependency standard | `sdkwork-specs/DEPLOYMENT_SPEC.md` §6.1 |
| Gateway application contract | `sdkwork-specs/APPLICATION_GATEWAY_SPEC.md` |

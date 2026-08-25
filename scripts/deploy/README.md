# sdkwork-api-cloud-gateway — Deploy Toolkit

Reusable, idempotent deployment for the standalone container gateway on
Ubuntu / WSL.

Packages **development / test / staging / production** with non-conflicting
host ports, **external PostgreSQL + Redis (no Redis password)**, and the same
FHS layout as Ubuntu install (`/etc/sdkwork/api_gateway/config.toml`).

> Public reverse proxy is **not** owned here. Configure
> `sdkwork-webserver` (independent Docker product) to forward
> `api-dev.*` / `api-test.*` / `api-staging.*` / `api.*` to the gateway host
> ports below.

## Design

| File | Responsibility |
| --- | --- |
| `lib/common.sh` | Logging, OS detection, docker/root helpers |
| `lib/environments.sh` | Env → port / database / domain matrix (single source of truth) |
| `prepare-host.sh` | Linux: install/start PostgreSQL+pgvector+Redis, provision DBs |
| `apply.sh` | Orchestrator: host-prep → build → deploy → health check (**no nginx**) |
| `apply.ps1` | Windows wrapper that delegates to WSL |

## Environment / port / database matrix

| Environment | Host port | Container | Database |
| --- | --- | --- | --- |
| development | 3910 | `:3900` | `sdkwork_ai_dev` |
| test | 3911 | `:3900` | `sdkwork_ai_test` |
| staging | 3912 | `:3900` | `sdkwork_ai_staging` |
| production | 3913 | `:3900` | `sdkwork_ai_prod` |

Default compose posture is **external** PostgreSQL + Redis
(`host.docker.internal:5432` / `:6380`, Redis password empty).

## Usage

```bash
sudo bash scripts/deploy/apply.sh                 # all envs
sudo bash scripts/deploy/apply.sh --skip-build
sudo bash scripts/deploy/apply.sh --skip-host
sudo bash scripts/deploy/apply.sh --env development test
```

Windows:

```powershell
pwsh scripts/deploy/apply.ps1
pwsh scripts/deploy/apply.ps1 -Env development,test
pwsh scripts/deploy/apply.ps1 -WslPassword Helloworld
```

## Verification

```bash
node scripts/validate-docker-deployment.mjs --matrix --compose
curl -fsS http://127.0.0.1:3910/healthz
```

Layout check:

```bash
node ../sdkwork-specs/tools/check-application-deploy-layout.mjs --root .
```

# sdkwork-api-cloud-gateway — Deploy Toolkit

Reusable, idempotent, multi-OS deployment for the standalone container gateway.

The toolkit packages the app for **development / test / staging / production** with
non-conflicting ports, external PostgreSQL + Redis, and **HTTP + HTTPS** ingress through a
single nginx on port 80/443 that routes by `Host` header to the right container.

> Convention: `api-dev.*` for development, `api-test.*` for test, `api-staging.*` for staging,
> and bare `api.*` for production. Every environment is served on all three public domains
> (`sdkwork.com`, `birdcoder.com`, `dtupay.com`).

## Design (high cohesion / low coupling)

| File | Responsibility |
| --- | --- |
| `lib/common.sh` | Logging, OS detection, docker/root helpers. Sourced everywhere. |
| `lib/environments.sh` | **Single source of truth** for the env→port/db/domain matrix. |
| `generate-nginx-conf.sh` | Renders `docker/nginx/<env>.conf` (80+443) for all envs/domains. |
| `generate-certs.sh` | Generates multi-SAN self-signed certs into `docker/certs/<env>/` (idempotent). |
| `install-certs.sh` | Installs certs to `/etc/ssl/sdkwork/api-gateway/<env>/` (root). |
| `apply.sh` | Orchestrator: host-prep → generate → build → deploy → install certs+nginx → health check. |
| `prepare-host.sh` | Linux only: install/start PostgreSQL+pgvector+Redis, issue PG TLS CA, provision per-env DBs. |
| `apply.ps1` | Windows wrapper that delegates to `apply.sh` inside WSL Ubuntu. |

Each script does exactly one thing and reads the matrix only from `environments.sh`, so there
is **no duplicated mapping** and changing the port/domain plan is a one-file edit.

## Environment / port / database matrix

| Environment | Subdomain | Host port | Container | Database (sdkwork-specs) |
| --- | --- | --- | --- | --- |
| development | `api-dev.{domain}` | 3910 | `:3900` | `sdkwork_ai_dev` |
| test | `api-test.{domain}` | 3911 | `:3900` | `sdkwork_ai_test` |
| staging | `api-staging.{domain}` | 3912 | `:3900` | `sdkwork_ai_staging` |
| production | `api.{domain}` | 3913 | `:3900` | `sdkwork_ai_prod` |

External dependency mode (`docker-compose.external.yml`) is the default: the composition disables
its built-in Postgres/Redis and the gateway reaches them through `host.docker.internal`
(WSL host-native services) or any remote managed endpoint. Each environment gets its own
database identity, matching `ENVIRONMENT_SPEC` §7.1 workspace identities.

## External PostgreSQL + Redis

Two external postures are supported by the same override:

1. **WSL host-native services** — PostgreSQL (5432) + Redis (6380) installed on Ubuntu; gateway
   reaches them via `host.docker.internal` (already set in `docker/env/*.env`).
2. **Remote managed services** — point the env file at the external host:
   ```
   GATEWAY_POSTGRES_HOST=db.internal.example.com
   GATEWAY_CLOUDROUTER_REDIS_HOST=redis.internal.example.com
   ```
   (then drop `GATEWAY_POSTGRES_PORT`/password as needed).

Provision the per-environment databases first:
```bash
export SDKWORK_DATABASE_ADMIN_HOST=127.0.0.1 SDKWORK_DATABASE_ADMIN_PORT=5432 \
       SDKWORK_DATABASE_ADMIN_USERNAME=postgres SDKWORK_DATABASE_ADMIN_PASSWORD=...
bash scripts/provision-external-databases.sh          # all four
# or: bash scripts/provision-external-databases.sh development test
```

## Host services (WSL / Ubuntu host-native posture)

`apply.sh` step 0 prepares the host so the external-dependency composition has something to
connect to. `prepare-host.sh` is idempotent and:

1. installs `postgresql` + `pgvector` + `redis-server` (skips if present);
2. starts them (systemd | `service` | `pg_ctlcluster` | `redis-server --daemonize`);
3. makes PostgreSQL listen on `0.0.0.0:5432` and accept docker-bridge clients;
4. issues a self-signed CA + server cert so `GATEWAY_POSTGRES_SSL_MODE=require` (production) validates,
   writing the CA into `docker/ca/postgres-ca-bundle.crt` (the gateway's trust mount);
5. provisions `sdkwork_ai_dev/test/staging/prod` via `provision-external-databases.sh`;
6. makes Redis bind `0.0.0.0` (no auth, matching the dev/test env files).

Run it standalone, or skip it when pointing the env files at a managed/cloud database:
```bash
sudo bash scripts/deploy/prepare-host.sh            # full host prep
sudo bash scripts/deploy/prepare-host.sh --no-tls   # plaintext PostgreSQL
sudo bash scripts/deploy/apply.sh --skip-host        # assume DB/Redis already run
```

## Usage

### Linux / macOS / WSL (native)
```bash
sudo bash scripts/deploy/apply.sh                 # full cycle (incl. host PG/Redis prep), all envs
sudo bash scripts/deploy/apply.sh --skip-build    # reuse existing image
sudo bash scripts/deploy/apply.sh --skip-host     # assume PostgreSQL/Redis already available
sudo bash scripts/deploy/apply.sh --env development test
sudo bash scripts/deploy/apply.sh --no-nginx       # containers only (no host nginx)
```

### Windows
```powershell
pwsh scripts/deploy/apply.ps1                      # delegates to WSL "Ubuntu"
pwsh scripts/deploy/apply.ps1 -SkipBuild
pwsh scripts/deploy/apply.ps1 -Env development,test
pwsh scripts/deploy/apply.ps1 -WslPassword Helloworld   # non-interactive sudo for certs/nginx
```

### Lower-level (still composable)
```bash
bash scripts/deploy/generate-nginx-conf.sh         # render site configs
bash scripts/deploy/generate-certs.sh              # generate self-signed certs
sudo bash scripts/deploy/install-certs.sh          # install certs to host
sudo bash scripts/install-nginx-sites.sh all        # install nginx sites + reload
bash scripts/deploy-docker-environment.sh all --validate
```

## TLS strategy

- Local/dev: self-signed multi-SAN certs from `generate-certs.sh` (covers all 3 domains per env).
- Production: replace with Let's Encrypt (ACME). The gateway already carries ACME profile config
  (`GATEWAY_WEBSERVER_ACME_*` in `docker/env/production.env`); mount the real `fullchain.pem` /
  `privkey.pem` at `/etc/ssl/sdkwork/api-gateway/production/` and the nginx `443` block picks them
  up with no config change.

## Verification
```bash
node scripts/validate-docker-deployment.mjs --matrix --compose   # matrix + compose sanity
curl -fsS http://127.0.0.1:3910/healthz                          # dev (direct)
curl -fsS https://api-dev.sdkwork.com/healthz                    # dev (HTTPS via nginx)
curl -fsS https://api.sdkwork.com/healthz                        # prod (HTTPS via nginx)
```

## Notes
- Re-running `apply.sh` is safe: compose `up -d` is idempotent, cert generation skips valid certs,
  and nginx configs are overwritten in place.
- On non-Linux hosts the nginx/cert steps are skipped automatically (nginx is a server concern);
  the gateway stays reachable on `127.0.0.1:<port>`.
- Windows `hosts` entries for `*.sdkwork.com / *.birdcoder.com / *.dtupay.com` → WSL IP:
  `powershell -ExecutionPolicy Bypass -File scripts/bind-windows-hosts.ps1`.

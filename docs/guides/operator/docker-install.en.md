# Docker Foolproof Install Guide (docker-install)

> Version: 2026-09-04 · Applies to: `sdkwork-api-cloud-gateway` standalone unified install image (bundle ≥ 0.1.1)
> Spec basis: `sdkwork-specs/DEPLOYMENT_SPEC.md` §6, `sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4
> 中文版：[docker-install.md](./docker-install.md)

## 0. Three Commands to a Running Gateway (TL;DR)

With the install bundle (the `.bundle` directory) on any Docker host.
**No bundle yet? Packaging is always step one — see §2 (repackaging scenarios
in §2.1)**. **External dependencies are the default**: the gateway uses YOUR PostgreSQL
and Redis instances via `GATEWAY_POSTGRES_*` / `GATEWAY_CLOUDROUTER_REDIS_*`
in the env file:

```bash
cp env/development.env.example env/development.env   # 1. Create the config
# Edit env/development.env: point GATEWAY_POSTGRES_* and
# GATEWAY_CLOUDROUTER_REDIS_* at your external PostgreSQL and Redis (§1 / §4)
bash deploy.sh --environment development             # 2. Deploy (external mode, the default)
curl http://127.0.0.1:3910/healthz                   # 3. Health check — ok means done
```

**For real deployments (especially production), continue with the
per-environment quick lanes (§0.1)** — production requires 7 mandatory
secrets and one forced TLS switch; missing any of them makes the gateway
refuse to start (fail-closed). No PostgreSQL/Redis at hand and just want a
30-second smoke test? Swap step 2 for
`bash deploy.sh --environment development --embedded` (bundled databases,
zero external deps).

### 0.1 Per-Environment Quick Lanes (copy & paste)

Each lane assumes the bundle is on the target host and you have `cd`-ed into
it (§3). **External mode is the default**: fill the external database/Redis
connection settings in the env file first, then deploy; add `--embedded` to
use the bundled databases instead.

**development (dev, external deps by default):**

```bash
cp env/development.env.example env/development.env
# Edit env/development.env: fill GATEWAY_POSTGRES_* / GATEWAY_CLOUDROUTER_REDIS_* (§4.2 has the full steps and examples)
bash deploy.sh --environment development             # external mode (default)
bash deploy.sh --environment development --embedded  # no external DB? bundled mode, zero config
curl http://127.0.0.1:3910/healthz
```

**test (external deps by default):**

```bash
cp env/test.env.example env/test.env
# Edit env/test.env: fill GATEWAY_POSTGRES_* / GATEWAY_CLOUDROUTER_REDIS_* (§4.2 has the full steps and examples)
bash deploy.sh --environment test                    # external mode (default)
bash deploy.sh --environment test --embedded         # or bundled mode for a quick start
curl http://127.0.0.1:3911/healthz
```

**staging (external database; built-in secrets are dev-only defaults, internal staging only):**

```bash
cp env/staging.env.example env/staging.env
# Edit env/staging.env: fill GATEWAY_POSTGRES_HOST/PORT/DB/USER/PASSWORD
# (full external PostgreSQL & Redis setup steps and examples in §4.2)
bash deploy.sh --environment staging            # external mode (default)
bash deploy.sh --environment staging --embedded # or built-in mode for a quick start
curl http://127.0.0.1:3912/healthz
```

**production (7 secrets + forced TLS — see §4.1 one-liner generators):**

```bash
cp env/production.env.example env/production.env
# ① Replace the 7 <CHANGE_ME> entries (1 database password + 6 Cloud Router
#    secrets — generation one-liners in §4.1; local-verification shortcut:
#    bash prepare-envs.sh --dev-secrets-for-production)
# ② External mode: fill GATEWAY_POSTGRES_* (sslmode=require is preset;
#    full external PostgreSQL & Redis setup steps and examples in §4.2)
bash deploy.sh --environment production --replicas 3   # or add --embedded
curl http://127.0.0.1:3913/healthz
```

> Running several environments on one machine? Fully supported (ports /
> networks / volumes / database names are isolated), but each environment
> must get a **different** `GATEWAY_IM_ID_NODE_ID` — see the matrix in §6.

## 1. Two Dependency Modes: How to Choose

One image, two postures, selected at deploy time. **External is the default**:

| | External mode (default) | Embedded mode (`--embedded`) |
| --- | --- | --- |
| PostgreSQL | Bring your own (managed or self-hosted) via `GATEWAY_POSTGRES_*` | Started automatically with the bundle (pgvector/pgvector:pg18, `vector` extension included, TLS auto-enabled) |
| Redis | Bring your own via `GATEWAY_CLOUDROUTER_REDIS_*` | Started automatically (redis:8-alpine, no password) |
| Best for | test / staging / production with a DBA or managed databases | demos, PoCs, isolated smoke tests, small deployments without external databases |
| Data lives in | Your database instance | Docker volumes `sdkwork-api-cloud-gateway-<env>-postgres-data` / `-redis-data` on the host |
| Upgrades / scaling | Stateless, `--replicas N` anytime | Also stateless; the built-in pair is single-instance and the capacity ceiling |

The rule is simple: **if you have decent PostgreSQL and Redis, use external
(the default); otherwise, or just for a quick look, use `--embedded`.** Both
modes share the same image and the same env file — switching modes is just
adding/removing one flag and re-running `deploy.sh`.

> Embedded mode is still isolated per environment: each environment gets its
> own network, database volumes, and database name
> (`sdkwork_ai_dev/test/...`), with no cross-talk.
> The embedded PostgreSQL also runs with TLS (certificates generated
> automatically); the connection sslmode follows `GATEWAY_POSTGRES_SSL_MODE`
> (`require` is preset in the production/staging examples).

## 2. Step 1: Package the Bundle (build machine)

**Everything starts with packaging**: no bundle, no deployment. The whole
pipeline is shell-only. One command at the repository root produces a
self-contained install bundle (image + compose + env templates + deploy
script). Four packaging actions — pick by what changed (detailed mapping in
the §2.1 table):

```bash
# ① Full pipeline: cargo build → OCI image → bundle assembly (all four steps)
#    Use when: anything that ends up inside the binary changed — Rust sources
#    (src/**), Cargo.toml dependencies, build.rs, embedded assets/migrations.
#    cargo is incremental (only changed sources recompile), far faster than a
#    cold build (~10–18 min cold).
bash scripts/build-install-bundle.sh

# ② Skip the compile: reuse the existing dist/server/ tarball (Rust artifacts
#    unchanged), still rebuild the image and repackage the bundle
#    Use when: Rust code and dependencies untouched, only image-level things
#    changed — Dockerfile / container build script
#    (build-api-cloud-gateway-container.mjs), base image choice, compose
#    templates, env templates, docs — anything that does not go into the binary.
bash scripts/build-install-bundle.sh --skip-server-package

# ③ Reassemble the bundle only: skip compile AND image build, done in seconds;
#    just re-assembles the bundle directory (deploy.sh / prepare-envs.sh /
#    compose / env templates / README / manifest copies + chmod; image.tar.gz
#    reused verbatim)
#    Use when: only the packaging/assembly layer changed —
#    scripts/docker/bundle/**, package-install-bundle.mjs,
#    build-install-bundle.sh itself, README.
#    Prerequisite: the <registry>:<version> image exists locally (the script
#    errors out rather than silently tagging something wrong).
bash scripts/build-install-bundle.sh --skip-server-package --skip-image-build

# ④ Dry run: combinable with ①②③; the bundle assembly step only prints the
#    commands it would run, writes nothing
#    Use when: verifying version / image ref / artifact paths before a release,
#    or debugging packaging configuration.
bash scripts/build-install-bundle.sh --dry-run
```

> **Two red lines for the reuse/skip flags**:
> ① `--skip-server-package` reuses the MOST RECENT tarball under dist/server/ —
> if the Rust code changed but you skipped the build, the stale binary goes
> into the image with no warning; when in doubt, run the full pipeline.
> ② Never use `--skip-image-build` after a Rust change (the new code would
> never reach the image — the bundle keeps shipping the old one);
> `--skip-image-build` should only be combined with `--skip-server-package`.

The script performs four steps: cargo release build + server tarball → OCI
image → registry tag from `release.currentVersion` in
`sdkwork.app.config.json` (currently 0.1.1) →
`dist/docker-install/sdkwork-api-cloud-gateway-install-<version>.bundle/`.
Build-machine prerequisites: Docker (with the compose plugin), Node.js, and
the Rust toolchain (cargo) — the packaging command itself is the single bash
line above.

### 2.1 Repackaging (picking skips for daily iteration)

Pick the skip flags by what actually changed this time:

| What changed | Command | Relative cost |
| --- | --- | --- |
| Rust code / Cargo dependencies | `bash scripts/build-install-bundle.sh` (full) | longest (compile + image) |
| Only packaging scripts / compose / env templates / docs | `bash scripts/build-install-bundle.sh --skip-server-package` | medium (image + bundle) |
| Only bundle assembly (deploy.sh / prepare-envs.sh / README) | `bash scripts/build-install-bundle.sh --skip-server-package --skip-image-build` | fastest (seconds) |
| Just want to see the plan | add `--dry-run` | seconds |

**Versioning**: before a release, bump `release.currentVersion` in
`sdkwork.app.config.json` (e.g. `0.1.2`), or override ad hoc with
`--version 0.1.2` — the version drives the image tag
(`registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:<version>`) and the
bundle directory name. **Each release keeps its own bundle directory, never
overwritten**; keep the previous one for rollback (§5.3).

**Pre-shipping checks**: the new bundle's `image.sha256` and
`manifest.json` record the image fingerprint — verify with `sha256sum -c`
after copying (§3). `prepare-envs.sh` never overwrites existing env files,
so repackaging never destroys server-side configuration.

Bundle layout (version comes from `sdkwork.app.config.json`; currently 0.1.1):

```text
dist/docker-install/sdkwork-api-cloud-gateway-install-<version>.bundle/
├── image.tar.gz / image.sha256 / image.env   # image archive + checksum + tag
├── compose/
│   ├── docker-compose.bundle.yml             # environment-neutral multi-instance template (external deps, default)
│   └── docker-compose.bundle.embedded.yml    # built-in PostgreSQL/Redis override (used by --embedded)
├── postgres/                                 # embedded-mode postgres init + TLS assets
├── env/                                      # 5 per-environment config templates
│   ├── development.env.example  test.env.example  staging.env.example
│   └── demo.env.example  production.env.example
├── deploy.sh                                 # the single deploy entrypoint
├── prepare-envs.sh                           # one-shot per-environment env creation (node ids auto-isolated)
├── manifest.json                             # version / image / sha256 metadata
└── README.md
```

## 3. Step 2: Copy to the Remote Server

Copy the whole `.bundle` directory to the target Linux host. The only
requirements there: **Docker installed (with the compose plugin)** and
**access to an image registry or the bundled image.tar.gz**.

Manual copy (scp or rsync — mind the trailing-slash semantics):

```bash
# scp (simplest)
scp -r dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.1.bundle \
    deploy@api.sdkwork.com:/opt/sdkwork/deploy/

# rsync (resumable, re-runs transfer only the diff; recommended for large bundles)
rsync -avP dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.1.bundle \
    deploy@api.sdkwork.com:/opt/sdkwork/deploy/
```

Verify integrity on the remote host after copying (mandatory for offline
hosts):

```bash
ssh deploy@api.sdkwork.com 'cd /opt/sdkwork/deploy/sdkwork-api-cloud-gateway-install-0.1.1.bundle \
    && sha256sum -c image.sha256'   # must print image.tar.gz: OK
```

Want to skip the manual copy? §8's one-command remote deploy runs the whole
"package → push → install → health check" chain for you.

## 4. Step 3: Configure (one file)

```bash
cp env/<environment>.env.example env/<environment>.env
```

If the env file is missing on first `deploy.sh` run, it is generated from the
example automatically. **Fields that actually matter** (defaults are fine for
everything else):

| Field | Meaning | External mode | Embedded mode |
| --- | --- | --- | --- |
| `GATEWAY_POSTGRES_HOST/PORT/DB/USER/PASSWORD` | External PostgreSQL connection | **required** | can be ignored |
| `GATEWAY_CLOUDROUTER_REDIS_HOST/PORT/PASSWORD` | External Redis connection | **required** | can be ignored |
| `GATEWAY_POSTGRES_SSL_MODE` | `require` is preset in production/staging examples (P0-12: production database connections must use TLS) | **required** in production | **required** in production (embedded TLS is wired automatically) |
| `GATEWAY_DEPS_MODE` | `external` (default) or `embedded` | leave alone | may set `embedded` instead of `--embedded` |
| `GATEWAY_IM_ID_NODE_ID` | Snowflake node id; **must differ across co-located environments** (§6) | recommended | recommended |
| `GATEWAY_CONTAINER_PORT` | Listener port *inside* the container, default `3900`; must be `8080` when attaching sdkwork-webserver (see below) | recommended | recommended |
| `GATEWAY_ALLOWED_HOSTS` | Gateway Host allowlist; must include the compose network aliases `gateway` and `sdkwork-api-cloud-gateway`, otherwise sidecar calls get `421 Misdirected Request` | **required** in production | **required** in production |
| `GATEWAY_CLOUDROUTER_API_KEY_PEPPER` + 5 more secrets | production placeholders MUST be replaced | required in production | required in production |

**The container port must be 8080 when sdkwork-webserver is attached.**
Imported webserver sidecars dial the canonical upstream
`sdkwork-api-cloud-gateway:8080` checkout-direct (never rewritten), so an
attached gateway deployment must (a) register the alias
`sdkwork-api-cloud-gateway` on its compose network (the bundle templates do
this) and (b) answer on 8080. Set `GATEWAY_CONTAINER_PORT=8080` in the env
file — the compose template then aligns the listener bind, the port mapping
and the readiness probe together, so **no extra override file is needed**.
The host port (`GATEWAY_HOST_PORT`, 3910/3911/3912/3913) is unaffected.

The Host allowlist must also admit the compose aliases (`normalize_host`
strips the port, so bare host names are enough):

```bash
# env/<environment>.env
GATEWAY_ALLOWED_HOSTS=...,gateway,sdkwork-api-cloud-gateway
```

**Password contains URL-reserved characters?** Do not put it in
`GATEWAY_POSTGRES_PASSWORD` directly; set a percent-encoded
`SDKWORK_DATABASE_URL` instead (the example files explain this). Note: an
explicit `SDKWORK_DATABASE_URL` in the env file wins over everything — in
embedded mode remove that line or point it at `postgres:5432`.

> If a `<CHANGE_ME>` placeholder is left as-is it **becomes the literal
> password/secret** (it works, but it is a weak credential). Fine for smoke
> tests; always replace in production (§4.1).

For external mode, create the per-environment database and schema ahead of
time (`sdkwork_ai_dev/test/staging/demo/prod`) with one shell command from
the repository (**full external PostgreSQL + Redis setup steps and examples
in §4.2**):

```bash
bash scripts/provision-external-databases.sh
```

### 4.1 One-Liner Generators for the Production Secrets

The production template contains **7 `<CHANGE_ME>` entries**. Run these on
the target host (openssl ships with every Docker host) and paste each output
into the matching line of `env/production.env`:

```bash
# ① Database password (external mode; change it on the database side too)
openssl rand -hex 24

# ② Five random secrets: API_KEY_PEPPER / TRUSTED_SUBJECT_SECRET /
#    APP_SESSION_SECRET / PAYMENT_WEBHOOK_SECRET / INTERNAL_GATEWAY_SIGNING_SECRET
#    (run once per secret, paste into the matching line)
openssl rand -hex 32

# ③ UPSTREAM_CREDENTIAL_KEY_RING (replace the whole line with this command's
#    output — one complete JSON line)
echo "{\"activeKeyId\":\"prod-v1\",\"activeKey\":\"$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')\",\"fingerprintKey\":\"$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')\",\"decryptionKeys\":[]}"
```

> Pick any `activeKeyId` (e.g. `prod-v1`); once live, **never rotate the
> published activeKey in place** (it invalidates issued credentials) — move
> the old key into `decryptionKeys` when rotating.

One command to prove nothing was missed (should print nothing):

```bash
grep -n "<CHANGE_ME>" env/production.env
```

The staging template ships dev-only defaults for the 6 Cloud Router secrets
(internal staging only); replace them the §4.1 way before exposing staging
externally.

### 4.2 External PostgreSQL & Redis: Setup Steps (commands + examples)

External mode (the default) requires an external PostgreSQL instance
(**with the pgvector extension**) and Redis reachable from the gateway, then
their connection settings go into the env file. Complete path from zero:

**Naming convention** (database = schema = role, followed by both the script
and the SQL below):

| Environment | Database / Schema / Role | Redis logical DBs (RTC / Web) |
| --- | --- | --- |
| development | `sdkwork_ai_dev` | 1 / 2 |
| test | `sdkwork_ai_test` | 3 / 4 |
| staging | `sdkwork_ai_staging` | 5 / 6 |
| demo | `sdkwork_ai_demo` | 9 / 10 |
| production | `sdkwork_ai_prod` | 7 / 8 |

**Step 1: provision the PostgreSQL server** (skip if you already have one;
it MUST have pgvector installed):

```bash
# Ubuntu: install PostgreSQL + pgvector (adjust the version, e.g. 14/16/18)
sudo apt install postgresql postgresql-14-pgvector

# Or run one via Docker (data persists in a named volume)
docker run -d --name sdkwork-pg --restart unless-stopped \
  -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=<admin-password> \
  -p 5432:5432 -v sdkwork-pg-data:/var/lib/postgresql/data \
  pgvector/pgvector:pg18
```

**Step 2: create the database / schema / extension / role** — pick one:

```bash
# Option A (recommended): one-shot script. Run from the repo root; fill
# GATEWAY_POSTGRES_PASSWORD in each env file first — the script uses it to
# create the same-named role.
export SDKWORK_DATABASE_ADMIN_HOST=<pg-host>
export SDKWORK_DATABASE_ADMIN_PORT=5432
export SDKWORK_DATABASE_ADMIN_USERNAME=postgres
export SDKWORK_DATABASE_ADMIN_PASSWORD='<admin-password>'
bash scripts/provision-external-databases.sh               # all five environments
bash scripts/provision-external-databases.sh production    # or just one

# Option B: manual SQL (DBA-reviewed environments). Three actions:
psql -h <pg-host> -U postgres -c "CREATE DATABASE sdkwork_ai_prod;"
# create the same-named schema + install pgvector inside it (superuser needed)
psql -h <pg-host> -U postgres -d sdkwork_ai_prod \
  -v db=sdkwork_ai_prod -v app_user=sdkwork_ai_prod \
  -f docker/postgres/external-schema.sql
# create the role and grant privileges
psql -h <pg-host> -U postgres -c \
  "CREATE ROLE sdkwork_ai_prod LOGIN PASSWORD '<gateway-postgres-password>';"
psql -h <pg-host> -U postgres -d sdkwork_ai_prod -c \
  "GRANT ALL PRIVILEGES ON DATABASE sdkwork_ai_prod TO sdkwork_ai_prod;"
```

**Step 3: provision Redis** (skip if you already have one):

```bash
# DEFAULT: no password (intranet / trusted network), AOF persistence only —
# this matches the env template default (GATEWAY_CLOUDROUTER_REDIS_PASSWORD= empty)
docker run -d --name sdkwork-redis --restart unless-stopped \
  -p 6380:6379 -v sdkwork-redis-data:/data \
  redis:8-alpine redis-server --appendonly yes

# Verify connectivity (expect PONG)
redis-cli -h <redis-host> -p 6380 ping
```

Optional hardening (only for public / cross-trust-boundary exposure) —
password-protected variant:

```bash
# Hardened: requirepass + AOF persistence
docker run -d --name sdkwork-redis --restart unless-stopped \
  -p 6380:6379 -v sdkwork-redis-data:/data \
  redis:8-alpine redis-server --requirepass '<redis-password>' --appendonly yes

# Verify connectivity (expect PONG)
redis-cli -h <redis-host> -p 6380 -a '<redis-password>' ping
```

Self-hosted or managed Redis both work as long as it is reachable (and the
password is known for the hardened variant). When multiple environments share
one Redis service, they are isolated by the logical DB numbers in the table
above; separate Redis instances per environment also work.

**Step 4: fill the connection settings into `env/<environment>.env`**
(production example):

```bash
# --- PostgreSQL (matches Step 2) ---
GATEWAY_POSTGRES_HOST=<pg-host>           # gateway runs in a container: host.docker.internal targets the host
GATEWAY_POSTGRES_PORT=5432
GATEWAY_POSTGRES_DB=sdkwork_ai_prod
GATEWAY_POSTGRES_SCHEMA=sdkwork_ai_prod
GATEWAY_POSTGRES_USER=sdkwork_ai_prod
GATEWAY_POSTGRES_PASSWORD=<role password from Step 2>
GATEWAY_POSTGRES_SSL_MODE=require         # forced for production/staging; dev/test on an intranet may use disable

# --- Redis variant 1 (default, no password): leave the password empty, URLs carry no password segment ---
GATEWAY_CLOUDROUTER_REDIS_HOST=<redis-host>
GATEWAY_CLOUDROUTER_REDIS_PORT=6380
GATEWAY_CLOUDROUTER_REDIS_PASSWORD=
GATEWAY_RTC_STATE_REDIS_URL=redis://<redis-host>:6380/7
GATEWAY_WEB_REDIS_URL=redis://<redis-host>:6380/8

# --- Redis variant 2 (optional hardening, with password): fill all three,
#     the password inside the URLs must be percent-encoded ---
# GATEWAY_CLOUDROUTER_REDIS_PASSWORD=<redis-password>
# GATEWAY_RTC_STATE_REDIS_URL=redis://:<url-encoded-redis-password>@<redis-host>:6380/7
# GATEWAY_WEB_REDIS_URL=redis://:<url-encoded-redis-password>@<redis-host>:6380/8
```

> Three things to watch:
> ① **When switching between the two variants, all three keys move together**:
> `GATEWAY_CLOUDROUTER_REDIS_PASSWORD`, `GATEWAY_RTC_STATE_REDIS_URL`, and
> `GATEWAY_WEB_REDIS_URL` must agree (no-password = empty password line +
> password-less URLs; with-password = all three filled).
> ② In the hardened variant the URLs are full URLs: special characters in the
> password (`@ : / # ?`) must be percent-encoded (one line with python3
> `urllib.parse.quote`); the logical DB numbers follow the table per
> environment.
> ③ The env templates default to `host.docker.internal` (reaching host-side
> services from inside the container); the compose templates add the
> `host-gateway` mapping automatically on Linux hosts, and a plain external
> hostname/IP works too.

**Step 5: pre-deploy connectivity checks** (pass both before `deploy.sh`):

```bash
psql "host=<pg-host> port=5432 dbname=sdkwork_ai_prod user=sdkwork_ai_prod \
  password=<role-password> sslmode=require" -c "SELECT extname FROM pg_extension WHERE extname='vector';"
# expect one row: vector

redis-cli -h <redis-host> -p 6380 ping                        # no-password variant, expect PONG
redis-cli -h <redis-host> -p 6380 -a '<redis-password>' ping  # hardened variant, expect PONG
```

## 5. Step 4: Deploy and Verify

`deploy.sh` is the only entrypoint and is idempotent:

```bash
bash deploy.sh --environment development                  # dev, single instance, external deps
bash deploy.sh --environment development --embedded       # same, built-in PostgreSQL + Redis
bash deploy.sh --environment production --replicas 3      # production, 3 instances (instance 1 migrates)

bash deploy.sh --environment test --ps                    # status
bash deploy.sh --environment test --logs 1                # follow instance 1 logs
bash deploy.sh --environment test --restart               # restart (deps → instances, waits for health)
bash deploy.sh --environment test --stop                  # stop (keeps containers and volumes)
bash deploy.sh --environment test --start                 # start after stop (waits for health)
bash deploy.sh --environment test --down                  # remove containers (keeps volumes)
bash deploy.sh --environment test --down --purge          # remove containers + volumes/network (embedded DB data is deleted too!)

bash deploy.sh --help                                     # all options
```

The script orchestrates the start order: built-in databases (if enabled) →
the `knowledgebase-rpc` singleton (provisions shared secrets/mTLS material) →
instance 1 (runs database migrations) → instances 2..N (migration skipped,
start only after instance 1 is healthy). A first-ever deploy (embedded
database init + full migrations) may take up to ~18 minutes; re-deploys are
idempotent and fast.

**Verify**:

```bash
curl http://127.0.0.1:<instance-port>/healthz    # liveness
curl http://127.0.0.1:<instance-port>/readyz     # readiness (first start waits for migrations, up to ~4 min)
```

Deploying inside WSL / Docker Desktop on Windows? The Windows browser can
open `http://localhost:<instance-port>/healthz` directly (verified working).
If the host has an HTTP proxy configured, always verify with
`curl --noproxy '*'` — otherwise results may be poisoned by the proxy.

### 5.1 Day-2 Operations (restart / stop / logs / file locations)

Every operations action also goes through `deploy.sh`, executed from the bundle
directory (on the remote host: `/opt/sdkwork/deploy/sdkwork-api-cloud-gateway`):

| Action | Command | Notes |
| --- | --- | --- |
| Status | `bash deploy.sh --environment <env> --ps` | Instance + deps container status and health |
| Logs | `bash deploy.sh --environment <env> --logs 1` | Follow instance 1 logs (`--logs 2` for instance 2, etc.) |
| Restart | `bash deploy.sh --environment <env> --restart` | Restarts deps → instances in order, **returns only after healthy** (non-zero exit on failure) |
| Stop | `bash deploy.sh --environment <env> --stop` | Stops all containers, **keeps containers and volumes**; `--start` recovers in seconds |
| Start | `bash deploy.sh --environment <env> --start` | Starts after a stop (deps → instances, waits for health); first deploy should use apply |
| Remove | `bash deploy.sh --environment <env> --down` | Removes containers/network, **keeps volumes** |
| Remove + wipe | `bash deploy.sh --environment <env> --down --purge` | Deletes volumes too (embedded database data is deleted!) |

> Difference between `--stop/--start/--restart` and `--down`: the former keeps
> containers (`docker stop/start/restart` semantics, instant restart), the
> latter removes containers and re-creates them on the next apply. Both keep
> volumes unless `--purge`.
> **Env file changes do NOT take effect via `--restart`** (restart only
> restarts processes, it does not re-create containers) — re-run apply once
> (idempotent): `bash deploy.sh --environment <env> --replicas N`.

**Same actions on the remote server** (run locally; the action is forwarded
to the remote deploy.sh):

```bash
bash scripts/docker/remote-deploy.sh --host <user@host> --environment production --restart
bash scripts/docker/remote-deploy.sh --host <user@host> --environment production --stop
bash scripts/docker/remote-deploy.sh --host <user@host> --environment production --ps
bash scripts/docker/remote-deploy.sh --host <user@host> --environment production --logs 1
```

**Native docker commands for logs and status** (equivalent, no deploy.sh needed):

```bash
docker ps --filter name=sdkwork-api-cloud-gateway                   # all related containers
docker logs -f --tail 200 sdkwork-api-cloud-gateway-<env>-i1-gateway-1              # instance 1 gateway logs
docker logs -f --tail 200 sdkwork-api-cloud-gateway-<env>-deps-knowledgebase-rpc-1  # deps singleton logs
docker compose -p sdkwork-api-cloud-gateway-<env>-i1 ps             # single instance project status
```

**Deployment files and configuration locations**:

| Item | Local (bundle directory) | Remote Ubuntu |
| --- | --- | --- |
| Deploy script | `<bundle>/deploy.sh` | `/opt/sdkwork/deploy/sdkwork-api-cloud-gateway/deploy.sh` |
| Environment config (the only file to edit) | `<bundle>/env/<env>.env` | `/etc/sdkwork/api-gateway/docker/<env>.env` |
| Per-instance overrides (optional) | `<bundle>/env/<env>.i<N>.env` | same directory `<env>.i<N>.env` |
| Compose templates | `<bundle>/compose/docker-compose.bundle.yml` (embedded mode: `docker-compose.bundle.embedded.yml`) | same bundle directory |
| Data volumes | Named Docker volumes (both modes: `sdkwork-api-cloud-gateway-<env>-secrets` / `-data`; embedded mode adds `-postgres-data` / `-redis-data` / `-postgres-tls`) — full list and in-container mapping in §5.2 | same names |
| Compose project names | instances `sdkwork-api-cloud-gateway-<env>-i<N>`; deps `sdkwork-api-cloud-gateway-<env>-deps` | same names |

### 5.2 Directory & Path Quick Reference (in-container ↔ host)

Use this when troubleshooting or backing up. `<env>` = `development` / `test` /
`staging` / `demo` / `production`.

**A. Common in-container paths** (inspect with
`docker exec -it <container> ls <path>`):

**gateway container** (`sdkwork-api-cloud-gateway-<env>-i<N>-gateway-1`; the
knowledgebase-rpc container in deps shares the same mounts):

| In-container path | Contents | Host-side counterpart |
| --- | --- | --- |
| `/run/secrets/sdkwork/api-gateway/` | Runtime keys & certs: `iam-signing-master.key`, `payment-credential-master.key`, `knowledgebase-rpc/` (mTLS server/client/CA trio + `caller-context.key`, `knowledgebase-secrets.key`, `im-launch-ticket.key`) | Docker volume `sdkwork-api-cloud-gateway-<env>-secrets` |
| `/var/lib/sdkwork/api-gateway/` | Gateway shared state; `knowledgebase-drive/` is the knowledgebase file store | Docker volume `sdkwork-api-cloud-gateway-<env>-data` |
| `/run/sdkwork/api-gateway/postgres-ca-bundle.crt` | Custom external-PostgreSQL CA (optional, plus the generated `.merged.crt` trust bundle) | Any host file, mounted read-only via env `GATEWAY_POSTGRES_CA_FILE_MOUNT` |
| `/opt/sdkwork/api-gateway/container/` | Container entrypoint scripts (e.g. `knowledgebase-rpc-entrypoint`) | Baked into the image, read-only |

**Embedded postgres container** (`--embedded` only:
`sdkwork-api-cloud-gateway-<env>-deps-postgres-1`):

| In-container path | Contents | Host-side counterpart |
| --- | --- | --- |
| `/var/lib/postgresql/` | PGDATA data directory (also holds the auto-generated `server.crt` / `server.key` TLS certs) | Docker volume `sdkwork-api-cloud-gateway-<env>-postgres-data` |
| `/var/run/sdkwork-postgres-tls/` | Auto-generated TLS certs (`server.crt` / `server.key`, mounted read-only into the gateway) | Docker volume `sdkwork-api-cloud-gateway-<env>-postgres-tls` |
| `/docker-entrypoint-initdb.d/` | First-boot init SQL (read-only) | bundle directory `postgres/init/` (bind mount) |
| `/opt/sdkwork/postgres/entrypoint.sh` | TLS-enabling entrypoint (read-only) | bundle directory `postgres/entrypoint.sh` (bind mount) |

**Embedded redis container** (`--embedded` only:
`sdkwork-api-cloud-gateway-<env>-deps-redis-1`):

| In-container path | Contents | Host-side counterpart |
| --- | --- | --- |
| `/data` | AOF persistence data | Docker volume `sdkwork-api-cloud-gateway-<env>-redis-data` |

**B. Host-side resources** (local = the machine holding the bundle; remote =
the Ubuntu target host):

| Resource | Local (bundle directory) | Remote Ubuntu |
| --- | --- | --- |
| Bundle install directory | `<bundle>/` (build machine: `dist/docker-install/...bundle`) | `/opt/sdkwork/deploy/sdkwork-api-cloud-gateway/` |
| Env config (the only file to edit) | `<bundle>/env/<env>.env` | `/etc/sdkwork/api-gateway/docker/<env>.env` (0600) |
| Docker network | `sdkwork-api-cloud-gateway-<env>` (external, pre-created by the script) | same name |
| Docker volume backing store | `/var/lib/docker/volumes/<volume>/_data` (requires root/sudo) | same |
| Container logs | Docker json-file default driver (50m × 3 rotated), via `docker logs` | same |

**Volume list** (isolated per environment; external mode uses only the first
two, embedded mode uses all five):

| Docker volume | In-container mount | Mode |
| --- | --- | --- |
| `sdkwork-api-cloud-gateway-<env>-secrets` | `/run/secrets/sdkwork/api-gateway` | external + embedded |
| `sdkwork-api-cloud-gateway-<env>-data` | `/var/lib/sdkwork/api-gateway` | external + embedded |
| `sdkwork-api-cloud-gateway-<env>-postgres-data` | `/var/lib/postgresql` | embedded only |
| `sdkwork-api-cloud-gateway-<env>-redis-data` | `/data` | embedded only |
| `sdkwork-api-cloud-gateway-<env>-postgres-tls` | `/var/run/sdkwork-postgres-tls` | embedded only |

**Quick lookup commands**:

```bash
docker volume ls --filter name=sdkwork-api-cloud-gateway                # all volumes for this deployment
docker volume inspect -f '{{.Mountpoint}}' \
  sdkwork-api-cloud-gateway-production-data                             # host-side backing path of a volume
docker exec sdkwork-api-cloud-gateway-production-i1-gateway-1 \
  ls /run/secrets/sdkwork/api-gateway                                   # list the secrets dir inside the container
```

> The volume backing directories live under `/var/lib/docker/volumes/`; reading
> them directly requires root. For routine backups use `pg_dump` (database,
> §5.3) or `docker run --rm -v <volume>:/data ... tar` (file volumes, §7).

### 5.3 Rollback Flow (app version / config / data)

**Do three things BEFORE every upgrade, or there is no way back**:

```bash
# ① Back up the current env config
cp /etc/sdkwork/api-gateway/docker/<env>.env \
   /etc/sdkwork/api-gateway/docker/<env>.env.bak.$(date +%F-%H%M)
# ② Keep the previous bundle (before rsyncing the new one over, copy the old
#    directory aside with a version suffix)
cp -r /opt/sdkwork/deploy/sdkwork-api-cloud-gateway \
      /opt/sdkwork/deploy/sdkwork-api-cloud-gateway-v0.1.1
# ③ Database backup: external mode — follow your own database procedure;
#    embedded mode — use the pg_dump command in "Data rollback" below
```

**① App version rollback (most common)**: every bundle ships its own
versioned `image.tar.gz`, so rollback = re-apply with the OLD bundle (same
compose project names, containers rebuilt idempotently):

```bash
cd /opt/sdkwork/deploy/sdkwork-api-cloud-gateway-v0.1.1     # previous-version bundle
bash deploy.sh --environment production \
  --env-file /etc/sdkwork/api-gateway/docker/production.env --replicas 3
curl --noproxy '*' http://127.0.0.1:3913/healthz            # 200 = rollback done
```

If both the old and new images are already on the machine (visible in
`docker images | grep sdkwork-api-cloud-gateway`), it is faster to point the
CURRENT bundle at the old tag — image resolution order: `--image-tag` >
bundle `image.env` > env `GATEWAY_IMAGE` > default version:

```bash
bash deploy.sh --environment production --image-tag 0.1.0 \
  --env-file /etc/sdkwork/api-gateway/docker/production.env --replicas 3
```

**② Config rollback**: restore the env backup, then re-run apply (idempotent,
config takes effect):

```bash
cp /etc/sdkwork/api-gateway/docker/<env>.env.bak.2026-09-03-1500 \
   /etc/sdkwork/api-gateway/docker/<env>.env
bash deploy.sh --environment <env> \
  --env-file /etc/sdkwork/api-gateway/docker/<env>.env --replicas N
```

**③ Data rollback (only for destructive migrations / corrupted data)**:
database migrations run **forward-only** — rolling the app version back does
NOT roll back the schema (the old version works against the newer schema), so
a normal rollback is just step ①. Restore the database only when a release
contained destructive migrations or data got corrupted:

```bash
# Backup (embedded mode; external mode is done on the database side, no
# gateway change needed)
docker exec sdkwork-api-cloud-gateway-<env>-deps-postgres-1 \
  pg_dump -U <user-same-as-db-name> -d <database-name> -Fc > pg-<env>-$(date +%F).dump

# Restore: stop first, restore, then start
bash deploy.sh --environment <env> --stop
cat pg-<env>-2026-09-03.dump | docker exec -i \
  sdkwork-api-cloud-gateway-<env>-deps-postgres-1 \
  pg_restore -U <user-same-as-db-name> -d <database-name> --clean --if-exists
bash deploy.sh --environment <env> --start
```

**④ Post-rollback verification checklist**:

```bash
bash deploy.sh --environment <env> --ps                    # instances + deps all healthy
curl --noproxy '*' http://127.0.0.1:<port>/healthz         # 200
curl --noproxy '*' http://127.0.0.1:<port>/readyz          # 200
docker logs --tail 100 sdkwork-api-cloud-gateway-<env>-i1-gateway-1   # no ERROR / crash loop
```

## 6. Multi-Environment Matrix (ports / node ids / databases)

Instance port formula: `instance i port = GATEWAY_HOST_PORT + (i-1) × 10 →
container 3900`. Multiple environments can share one machine — ports,
networks, volumes, and databases are isolated by design; **but the snowflake
node id `GATEWAY_IM_ID_NODE_ID` must differ per environment**, otherwise
co-located fleets may mint duplicate ids:

| Environment | Base port (instance 1) | Suggested node id | Database / Schema | Redis logical DBs | Example ports (3 instances) |
| --- | --- | --- | --- | --- | --- |
| development | 3910 | 21 | `sdkwork_ai_dev` | 1 / 2 | 3910 / 3920 / 3930 |
| test | 3911 | 22 | `sdkwork_ai_test` | 3 / 4 | 3911 / 3921 / 3931 |
| staging | 3912 | 23 | `sdkwork_ai_staging` | 5 / 6 | 3912 / 3922 / 3932 |
| production | 3913 | 24 | `sdkwork_ai_prod` | 7 / 8 | 3913 / 3923 / 3933 |
| demo | 3914 | 25 | `sdkwork_ai_demo` | 9 / 10 | 3914 / 3924 / 3934 |

Each environment's configuration lives in one env file shared by all its
instances: `env/<environment>.env` inside the bundle, or
`/etc/sdkwork/api-gateway/docker/<environment>.env` on remote Ubuntu. For
per-instance overrides, create `env/<environment>.i<index>.env` and it is
layered on automatically.

The public reverse proxy is owned by the independent `sdkwork-webserver`
deployment: point each environment's domain upstreams at that environment's
instance port list (e.g. production `api.{sdkwork,birdcoder,dtupay}.com` →
`127.0.0.1:3913; 3923; 3933`). Gateway-side host allowlist / CORS values are
pre-populated per environment in the env examples.

**Recreate multiple environments on one host in one shot**: the bundle ships
`prepare-envs.sh` — it creates every environment's env file, assigns the
distinct node ids from the §6 table automatically, and never overwrites an
existing env file (safe to re-run after bundle upgrades):

```bash
bash prepare-envs.sh                               # create missing env/<environment>.env (idempotent)
bash prepare-envs.sh --dev-secrets-for-production  # local verification only: fill production secrets from dev defaults (never real production)
bash prepare-envs.sh --force                       # reset to example values (discards manual edits, careful)
```

## 7. Embedded Mode Details

- **What starts**: two extra containers in the deps project — `postgres`
  (pgvector image, `ssl=on`, certificates generated automatically, schema
  created automatically) and `redis` (no password). The gateway and
  knowledgebase-rpc are repointed at them automatically; no
  `GATEWAY_POSTGRES_*` / `GATEWAY_CLOUDROUTER_REDIS_*` edits needed.
- **Data persistence**: data lives in Docker volumes
  `sdkwork-api-cloud-gateway-<env>-postgres-data` / `-redis-data`
  (full volume names and in-container mapping in §5.2); `--down` keeps them,
  `--down --purge` deletes them. Backup:
  `docker run --rm -v sdkwork-api-cloud-gateway-<env>-postgres-data:/data -v $PWD:/backup alpine tar czf /backup/pg.tar.gz /data`.
- **Migrating back to external**: pg_dump out of the built-in database,
  restore into your external PostgreSQL, then drop `--embedded`, point
  `GATEWAY_POSTGRES_*` at the new instance, and re-run `deploy.sh`.
- **Limits**: the built-in pair is single-instance (no replication/clustering)
  and not meant for production-scale capacity or HA; production should use
  external mode.
- Embedded production/staging enforce database TLS the same way: the examples
  preset `GATEWAY_POSTGRES_SSL_MODE=require`, and the embedded database's TLS
  is wired by compose automatically — no manual steps.
- If `SDKWORK_DATABASE_URL` is set explicitly in the env file it wins — in
  embedded mode remove that line or point it at `postgres:5432`.

## 8. One-Command Remote Deploy (package → push → install → health check)

`scripts/docker/remote-deploy.sh` chains §2–§5 into a single command: package
locally (or reuse an existing bundle) → push over SSH → materialize the
remote config → run the remote deploy.sh → health check. Remote
configuration wins and is never overwritten by re-deploys:

```bash
# Prerequisite: run `ssh-copy-id deploy@api.sdkwork.com` once (passwordless login)
bash scripts/docker/remote-deploy.sh --host deploy@api.sdkwork.com \
    --environment production --replicas 3

# Remote deploy with built-in dependencies (no external PostgreSQL/Redis needed)
bash scripts/docker/remote-deploy.sh --host deploy@api.sdkwork.com \
    --environment development --embedded

# Common options: reuse an existing bundle / explicit key / dry-run (prints the commands only)
bash scripts/docker/remote-deploy.sh --host deploy@api.sdkwork.com \
    --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.1.bundle \
    --ssh-key ~/.ssh/prod_ed25519 --environment test --replicas 2 --dry-run
```

Remote config file: `/etc/sdkwork/api-gateway/docker/<environment>.env`
(mode 0600, generated on first push). Change passwords / databases by editing
that file and re-running the same command. Remote passwordless sudo
(`sudo -n`) is required; otherwise pass `--config-root` pointing at a
user-writable directory. Day-2 operations go through the same script:
`--ps` / `--logs N` / `--restart` / `--stop` / `--start` / `--down` /
`--down --purge` are forwarded to the remote deploy.sh (see §5.1).

## 9. Troubleshooting Quick Table + FAQ

### 9.1 Symptom Quick Table (match the row, then read the FAQ)

| Symptom | Most likely cause | Fix |
| --- | --- | --- |
| Container `Restarting` loop, logs say `must contain sslmode=require` | production/staging enforce database TLS (P0-12 fail-closed); connection string is plaintext | Ensure `GATEWAY_POSTGRES_SSL_MODE=require` in the env (external: point at a TLS-capable database; embedded: handled by the template); re-run `deploy.sh` |
| Container `Restarting` loop, logs say `<X> is required for ...` | A fail-closed secret is missing (e.g. the 6 Cloud Router secrets) | Fill the missing keys in the env (§4 / §4.1), redeploy |
| `port is already allocated` | Port taken by an old fleet or another environment | `docker ps --format '{{.Names}}\t{{.Ports}}'` to find the occupier; change `GATEWAY_HOST_PORT` or clean up |
| `/healthz` unreachable although the container is healthy | Local proxy hijacks curl | `curl --noproxy '*' http://127.0.0.1:<port>/healthz` |
| Deploy command "succeeds" but containers are not up | Exit code swallowed by a pipe (false pass) | Always prove with a real `curl /healthz` afterwards — never trust the exit code alone |
| Multi-instance: only instance 1 starts | Instance 1 migration still running; 2..N are waiting | Wait for `/readyz` 200 (up to ~4 min); the rest follow automatically |
| Image tag wrong after `docker load` | Manually imported OCI archive without a tag | `docker tag <imageID> <ref from manifest.json>` |

### 9.2 FAQ

**Q: `curl /healthz` fails?**
In order: `bash deploy.sh --environment <env> --ps` — are containers healthy?
→ `--logs 1` for startup logs (match crash loops against §9.1) → external
mode: is `GATEWAY_POSTGRES_HOST` reachable from inside the container (a
database on the host uses `host.docker.internal`, which is the env example
default) → first start runs migrations; `/readyz` may take up to 4 minutes.

**Q: How do I change the database password?**
Update `GATEWAY_POSTGRES_PASSWORD` (or the whole `SDKWORK_DATABASE_URL`) in
the env file → re-run `deploy.sh --environment <env>`. Remember to change the
password on the database side too.

**Q: Scaling out / in?**
Re-run with a different `--replicas N` (idempotent). New instances skip
migration automatically; scaling in just stops the surplus instance projects.

**Q: What does `--down --purge` delete?**
The environment's network + secrets/data volumes; in embedded mode it also
deletes the built-in database volumes (**unrecoverable**). External mode never
touches your databases.

**Q: Can multiple environments share one machine?**
Yes. Ports, networks, volumes, and database names are all isolated per
environment — see the matrix in §6; remember to give each environment a
different `GATEWAY_IM_ID_NODE_ID`.

**Q: No image registry available?**
`image.tar.gz` is the image itself; deploy.sh runs `docker load`
automatically when the image is missing — fully offline capable (verify with
`sha256sum -c image.sha256` first, see §3).

**Q: How do I upgrade to a newer bundle?**
Data lives in external databases or Docker volumes, upgrades keep it:
`--down` the old version → `deploy.sh` from the new bundle → verify with
`/healthz`. When the image changed, deploy.sh loads the new one per the tag
in `image.env` automatically.

## 10. Operations Checklist

- [ ] `manifest.json` image sha256 matches `image.sha256` (offline installs: also `sha256sum -c image.sha256`)
- [ ] `/healthz` and `/readyz` pass for every environment (use `--noproxy '*'` to rule out proxy interference)
- [ ] Under multi-instance deployment only instance 1 shows migration logs
- [ ] Co-located environments use distinct `GATEWAY_IM_ID_NODE_ID` values
- [ ] All `<CHANGE_ME>` placeholders replaced in staging/production env files (1 database password + 6 Cloud Router secrets; `grep -n "<CHANGE_ME>" env/<env>.env` prints nothing)
- [ ] `GATEWAY_POSTGRES_SSL_MODE=require` for staging/production (both external and embedded modes, P0-12)
- [ ] Explicit `SDKWORK_DATABASE_URL` overrides match the dependency mode (embedded: `postgres:5432` or removed)
- [ ] Every environment domain reaches the gateway API through sdkwork-webserver

**Upgrade / rollback readiness** (run through before every upgrade, see §5.3):

- [ ] env config backed up (`<env>.env.bak.<timestamp>`)
- [ ] Previous-version bundle kept (version-suffixed directory) with its `image.tar.gz` intact
- [ ] Database backed up as of now (external: own procedure; embedded: §5.3 pg_dump)
- [ ] Rollback drilled once via `--image-tag <old-version>` or old-bundle apply

Remote deploy operations guide (default host `ubuntu@api.sdkwork.com`):
`docs/guides/operator/REMOTE_DEPLOY_BUNDLE.en.md`;
bare-metal Ubuntu (systemd) install: `docs/guides/operator/INSTALLATION_STANDARDS.md`.

# Docker Foolproof Install Guide (docker-install)

> Version: 2026-09-03 · Applies to: `sdkwork-api-cloud-gateway` standalone unified install image (bundle ≥ 0.1.1)
> Spec basis: `sdkwork-specs/DEPLOYMENT_SPEC.md` §6, `sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4
> 中文版：[docker-install.md](./docker-install.md)

## 0. Three Commands to a Running Gateway (TL;DR)

With the install bundle (the `.bundle` directory) on any Docker host:

```bash
cp env/development.env.example env/development.env   # 1. Create config (defaults are fine for a smoke test)
bash deploy.sh --environment development --embedded  # 2. Deploy (built-in databases, zero external deps)
curl http://127.0.0.1:3910/healthz                   # 3. Health check — ok means done
```

That's it. **Use the path above for a 30-second look; for real deployments
(especially production), continue with the per-environment quick lanes
(§0.1)** — production requires 7 mandatory secrets and one forced TLS switch;
missing any of them makes the gateway refuse to start (fail-closed).

### 0.1 Per-Environment Quick Lanes (copy & paste)

Each lane assumes the bundle is on the target host and you have `cd`-ed into
it (§3).

**development (dev, built-in deps, zero config):**

```bash
cp env/development.env.example env/development.env
bash deploy.sh --environment development --embedded
curl http://127.0.0.1:3910/healthz
```

**test (built-in deps, zero config):**

```bash
cp env/test.env.example env/test.env
bash deploy.sh --environment test --embedded
curl http://127.0.0.1:3911/healthz
```

**staging (external database; built-in secrets are dev-only defaults, internal staging only):**

```bash
cp env/staging.env.example env/staging.env
# Edit env/staging.env: fill GATEWAY_POSTGRES_HOST/PORT/DB/USER/PASSWORD
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
# ② External mode: fill GATEWAY_POSTGRES_* (sslmode=require is preset)
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
| Data lives in | Your database instance | Docker volumes `<env>-postgres-data` / `<env>-redis-data` on the host |
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

One command at the repository root produces a self-contained install bundle
(image + compose + env templates + deploy script):

```bash
pnpm build:container:install                        # build image + package
pnpm build:container:install -- --skip-image-build  # reuse a built image, repackage only
pnpm build:container:install -- --dry-run           # print the plan only
```

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

## 3. Step 2: Copy to the Target Host

Copy the whole `.bundle` directory to the target Linux host (scp / rsync /
USB — anything works). The only requirements: **Docker installed (with the
compose plugin)** and **access to an image registry or the bundled
image.tar.gz**.

Fully offline install (no network at all): verify before deploying —

```bash
sha256sum -c image.sha256    # must print image.tar.gz: OK
```

Want to skip the manual copy? Jump to §8 one-command remote deploy.

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
| `GATEWAY_CLOUDROUTER_API_KEY_PEPPER` + 5 more secrets | production placeholders MUST be replaced | required in production | required in production |

**Password contains URL-reserved characters?** Do not put it in
`GATEWAY_POSTGRES_PASSWORD` directly; set a percent-encoded
`SDKWORK_DATABASE_URL` instead (the example files explain this). Note: an
explicit `SDKWORK_DATABASE_URL` in the env file wins over everything — in
embedded mode remove that line or point it at `postgres:5432`.

> If a `<CHANGE_ME>` placeholder is left as-is it **becomes the literal
> password/secret** (it works, but it is a weak credential). Fine for smoke
> tests; always replace in production (§4.1).

For external mode, create the per-environment database and schema ahead of
time (`sdkwork_ai_dev/test/staging/demo/prod`); from the repository,
`pnpm db:postgres:provision` does it in one shot.

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

## 5. Step 4: Deploy and Verify

`deploy.sh` is the only entrypoint and is idempotent:

```bash
bash deploy.sh --environment development                  # dev, single instance, external deps
bash deploy.sh --environment development --embedded       # same, built-in PostgreSQL + Redis
bash deploy.sh --environment production --replicas 3      # production, 3 instances (instance 1 migrates)

bash deploy.sh --environment test --ps                    # status
bash deploy.sh --environment test --logs 1                # follow instance 1 logs
bash deploy.sh --environment test --down                  # stop (keeps volumes)
bash deploy.sh --environment test --down --purge          # stop + delete volumes/network (embedded DB data is deleted too!)

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
  `<env>-postgres-data` / `<env>-redis-data`; `--down` keeps them,
  `--down --purge` deletes them. Backup:
  `docker run --rm -v <env>-postgres-data:/data -v $PWD:/backup alpine tar czf /backup/pg.tar.gz /data`.
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

## 8. One-Command Remote Ubuntu Deploy (SSH push)

Package locally, then push and deploy on the remote host. Remote
configuration wins and is never overwritten by re-deploys:

```bash
# Prerequisite: run `ssh-copy-id deploy@1.2.3.4` once (passwordless login)
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --environment production --replicas 3

# Remote deploy with built-in dependencies
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --environment development --embedded

# Common options: reuse an existing bundle / explicit key / dry-run
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.1.bundle \
    --ssh-key ~/.ssh/prod_ed25519 --environment test --replicas 2 --dry-run
```

Remote config file: `/etc/sdkwork/api-gateway/docker/<environment>.env`
(mode 0600, generated on first push). Change passwords / databases by editing
that file and re-running the same command. Remote passwordless sudo
(`sudo -n`) is required; otherwise pass `--config-root` pointing at a
user-writable directory.

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

Full operator manual: `docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md`;
bare-metal Ubuntu (systemd) install: `docs/guides/operator/INSTALLATION_STANDARDS.md`.

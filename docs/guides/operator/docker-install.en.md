# Docker Foolproof Install Guide (docker-install)

> Version: 2026-09-02 · Applies to: `sdkwork-api-cloud-gateway` standalone unified install image
> Spec basis: `sdkwork-specs/DEPLOYMENT_SPEC.md` §6, `sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4
> 中文版：[docker-install.md](./docker-install.md)

## 0. Three Commands to a Running Gateway (TL;DR)

With the install bundle (the `.bundle` directory) on any Docker host:

```bash
cp env/development.env.example env/development.env   # 1. Create config (defaults are fine for a smoke test)
bash deploy.sh --environment development             # 2. Deploy (external databases, the default)
curl http://127.0.0.1:3910/healthz                   # 3. Health check — ok means done
```

No PostgreSQL / Redis handy? Add `--embedded` and the built-in databases start with the bundle:

```bash
bash deploy.sh --environment development --embedded
```

That's it. The rest of this guide covers which mode to pick, per-environment
configuration, scaling, and troubleshooting.

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

## 2. Step 1: Package the Bundle (build machine)

One command at the repository root produces a self-contained install bundle
(image + compose + env templates + deploy script):

```bash
pnpm build:container:install                        # build image + package
pnpm build:container:install -- --skip-image-build  # reuse a built image, repackage only
pnpm build:container:install -- --dry-run           # print the plan only
```

Bundle layout:

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
├── manifest.json                             # version / image / sha256 metadata
└── README.md
```

## 3. Step 2: Copy to the Target Host

Copy the whole `.bundle` directory to the target Linux host (scp / rsync /
USB — anything works). The only requirements: **Docker installed (with the
compose plugin)** and **access to an image registry or the bundled
image.tar.gz**.

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
| `GATEWAY_POSTGRES_SSL_MODE` | `require` recommended for staging/production (preset in the examples) | recommended | TLS enabled automatically |
| `GATEWAY_DEPS_MODE` | `external` (default) or `embedded` | leave alone | may set `embedded` instead of `--embedded` |
| `GATEWAY_CLOUDROUTER_API_KEY_PEPPER` + 5 more secrets | production `<CHANGE_ME>` placeholders MUST be replaced | required in production | required in production |

**Password contains URL-reserved characters?** Do not put it in
`GATEWAY_POSTGRES_PASSWORD` directly; set a percent-encoded
`SDKWORK_DATABASE_URL` instead (the example files explain this).

For external mode, create the per-environment database and schema ahead of
time (`sdkwork_ai_dev/test/staging/demo/prod`); from the repository,
`pnpm db:postgres:provision` does it in one shot.

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
start only after instance 1 is healthy).

**Verify**:

```bash
curl http://127.0.0.1:<instance-port>/healthz    # liveness
curl http://127.0.0.1:<instance-port>/readyz     # readiness (first start waits for migrations, up to ~4 min)
```

## 6. Multi-Environment Matrix (ports / databases)

Instance port formula: `instance i port = GATEWAY_HOST_PORT + (i-1) × 10 →
container 3900`. Multiple environments can share one machine — ports and
databases are isolated by design:

| Environment | Base port (instance 1) | Database / Schema | Redis logical DBs | Example ports (3 instances) |
| --- | --- | --- | --- | --- |
| development | 3910 | `sdkwork_ai_dev` | 1 / 2 | 3910 / 3920 / 3930 |
| test | 3911 | `sdkwork_ai_test` | 3 / 4 | 3911 / 3921 / 3931 |
| staging | 3912 | `sdkwork_ai_staging` | 5 / 6 | 3912 / 3922 / 3932 |
| production | 3913 | `sdkwork_ai_prod` | 7 / 8 | 3913 / 3923 / 3933 |
| demo | 3914 | `sdkwork_ai_demo` | 9 / 10 | 3914 / 3924 / 3934 |

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
    --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.0.bundle \
    --ssh-key ~/.ssh/prod_ed25519 --environment test --replicas 2 --dry-run
```

Remote config file: `/etc/sdkwork/api-gateway/docker/<environment>.env`
(mode 0600, generated on first push). Change passwords / databases by editing
that file and re-running the same command. Remote passwordless sudo
(`sudo -n`) is required; otherwise pass `--config-root` pointing at a
user-writable directory.

## 9. FAQ

**Q: `curl /healthz` fails?**
In order: `bash deploy.sh --environment <env> --ps` — are containers healthy?
→ `--logs 1` for startup logs → external mode: is `GATEWAY_POSTGRES_HOST`
reachable from inside the container (a database on the host uses
`host.docker.internal`, which is the env example default) → first start runs
migrations; `/readyz` may take up to 4 minutes.

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
environment — see the matrix in §6.

**Q: No image registry available?**
`image.tar.gz` is the image itself; deploy.sh runs `docker load`
automatically when the image is missing — fully offline capable.

## 10. Operations Checklist

- [ ] `manifest.json` image sha256 matches `image.sha256`
- [ ] `/healthz` and `/readyz` pass for every environment
- [ ] Under multi-instance deployment only instance 1 shows migration logs
- [ ] All `<CHANGE_ME>` placeholders replaced in staging/production env files (including the 6 Cloud Router secrets)
- [ ] `GATEWAY_POSTGRES_SSL_MODE=require` for staging/production (external mode)
- [ ] Every environment domain reaches the gateway API through sdkwork-webserver

Full operator manual: `docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md`;
bare-metal Ubuntu (systemd) install: `docs/guides/operator/INSTALLATION_STANDARDS.md`.

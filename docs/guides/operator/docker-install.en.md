# Docker Install Bundle Deployment Guide (docker-install)

> Version: 2026-08-30 · Applies to: `sdkwork-api-cloud-gateway` standalone unified install image
> Spec basis: `sdkwork-specs/DEPLOYMENT_SPEC.md` §6, `sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4

## 1. Design Summary

`pnpm build:container:install` produces **one unified install bundle**
(self-contained install bundle):

- **One image**: the image is environment- and instance-count-neutral. Builds
  **never** bake environment, domain, database, or credential values; the
  lifecycle environment (development / test / staging / production) and the
  instance count are **deployment-time inputs** resolved by the container
  entrypoint at start.
- **Any environment**: development, test, staging, and production share the
  same image tag; the env file selects the environment (`GATEWAY_ENVIRONMENT`).
- **Every environment supports N instances**: N gateway instances share one
  network (`sdkwork-api-cloud-gateway-<env>`) and one secrets/data volume pair;
  each instance owns its own compose project
  (`sdkwork-api-cloud-gateway-<env>-i<index>`) and its own host port. Instance 1
  runs the database migration first; instances 2..N start only after it is
  healthy.
- **Externalized dependencies**: the bundle ships no embedded PostgreSQL/Redis;
  `GATEWAY_POSTGRES_*` / `GATEWAY_CLOUDROUTER_REDIS_*` point at external
  instances. The public reverse proxy is owned by the independent
  `sdkwork-webserver` deployment — this bundle installs no nginx.

## 2. Packaging the Bundle

```bash
# Repository root (sdkwork-api-cloud-gateway)
pnpm build:container:install                       # build image + package
pnpm build:container:install -- --skip-image-build # reuse an existing image
pnpm build:container:install -- --tag 0.1.0 --out dist/docker-install --dry-run
```

From the sdkwork-space root (WSL / CI) use the `bin` script:

```bash
bash bin/build-gateway-docker.sh                   # reads the manifest version
bash bin/build-gateway-docker.sh --out /opt/deploy/packages
```

Bundle layout:

```text
dist/docker-install/sdkwork-api-cloud-gateway-install-<version>.bundle/
├── image.tar.gz / image.sha256 / image.env   # image archive + checksum + tag
├── compose/
│   └── docker-compose.bundle.yml             # environment-neutral multi-instance template
├── env/
│   ├── development.env.example
│   ├── test.env.example
│   ├── staging.env.example
│   └── production.env.example
├── deploy.sh                                 # single generic entrypoint
├── manifest.json                             # version / image / sha256 metadata
└── README.md
```

## 3. Deploying (any Docker host)

Prerequisites: an external PostgreSQL (pgvector extension recommended) and
Redis reachable from the containers (`host.docker.internal` or a managed
endpoint), with per-environment databases and schemas created
(`sdkwork_ai_dev/test/staging/prod`; `pnpm db:postgres:provision` can do this).

After copying the bundle directory to the target host, `deploy.sh` is the only
entrypoint:

```bash
# Development, single instance (default)
bash deploy.sh --environment development

# Production with 3 instances
bash deploy.sh --environment production --replicas 3

# Other operations
bash deploy.sh --environment test --ps
bash deploy.sh --environment test --logs 2        # follow instance 2 logs
bash deploy.sh --environment test --down
bash deploy.sh --environment test --down --purge  # also delete volumes/network
```

Key rules:

- `--environment` is required (development | test | staging | production); a
  missing or unknown value fails **before any side effect**. Re-running apply
  is idempotent (updates the existing stack in place).
- First deployment creates `env/<environment>.env` from the example — **fill
  PostgreSQL, Redis, and Cloud Router secrets before exposing anything**.
- If the image is missing, the script runs `docker load image.tar.gz`.
- Start order: one `knowledgebase-rpc` singleton per environment (the deps
  project, provisions shared mTLS/secret material) → gateway instance 1 (runs
  `--migrate-databases`) → instances 2..N (`GATEWAY_MIGRATE_ON_START=false`,
  avoiding migration races).

Repository-root equivalent:

```bash
pnpm deploy:apply:standalone:docker -- --environment production --replicas 3
pnpm deploy:apply:standalone:docker -- --environment development --down
```

## 4. Port Plan and sdkwork-webserver Integration

Instance port formula (deterministic, collision-free across environments):

```text
instance i host port = GATEWAY_HOST_PORT + (i - 1) * 10   ->  container 3900
```

| Environment | Base port (instance 1) | Database / Schema | Example ports (3 instances) |
| --- | --- | --- | --- |
| development | 3910 | `sdkwork_ai_dev` | 3910 / 3920 / 3930 |
| test | 3911 | `sdkwork_ai_test` | 3911 / 3921 / 3931 |
| staging | 3912 | `sdkwork_ai_staging` | 3912 / 3922 / 3932 |
| production | 3913 | `sdkwork_ai_prod` | 3913 / 3923 / 3933 |

Health: `curl http://127.0.0.1:<instance-port>/healthz` (readiness: `/readyz`).

`sdkwork-webserver` is an independent Docker deployment owning the public
reverse proxy: point each environment's API upstreams (`/app/`, `/backend/`,
...) at that environment's gateway instance ports for load balancing (for
production domains `api.{sdkwork,birdcoder,dtupay}.com` →
`127.0.0.1:3913; 127.0.0.1:3923; 127.0.0.1:3933`). The gateway-side
`GATEWAY_ALLOWED_HOSTS` / `GATEWAY_CORS_ALLOWED_ORIGINS` must include the
corresponding domains (pre-populated per environment in the env files).

## 5. Multi-Instance Notes

- **Migration ownership**: only instance 1 migrates (the entrypoint's
  `--migrate-databases` lifecycle step). Scaling out (re-running apply with a
  higher `--replicas`) starts new instances with migration skipped.
- **Shared secrets**: the secrets volume is shared per environment. The first
  boot provisions the payment master key / IAM signing master key /
  Knowledgebase RPC mTLS material with noclobber semantics, so concurrent
  first boots are safe (exactly one process creates each key). For stricter
  compliance, set `GATEWAY_PROVISION_*` to `false` in the env file and mount
  operator/KMS-managed keys.
- **Stateless horizontal scaling**: instances share PostgreSQL and Redis (RTC
  state and web sessions use Redis DB indexes), so `--replicas N` can rebuild
  or shrink at any time.
- **Per-instance overrides**: create `env/<environment>.i<index>.env` for
  instance-specific values; deploy.sh layers it on top of the base env file
  (the later `--env-file` wins).
- **Resource limits**: each instance defaults to 2 CPUs / 2 GiB; tune via
  `GATEWAY_CPU_LIMIT` / `GATEWAY_MEMORY_LIMIT`.

## 6. Operations and Verification Checklist

```bash
bash deploy.sh --environment <env> --ps        # instance + knowledgebase-rpc status
bash deploy.sh --environment <env> --logs 1    # follow instance logs
bash deploy.sh --environment <env> --down      # stop (keeps volumes/network)
```

- [ ] `docker compose -f compose/docker-compose.bundle.yml config` has no interpolation errors
- [ ] `/healthz` and `/readyz` pass for every environment
- [ ] Under multi-instance deployment only instance 1 shows migration logs
- [ ] Every environment domain reaches the gateway API through sdkwork-webserver
- [ ] `manifest.json` image sha256 matches `image.sha256`

Full operator manual: `docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md`.

## 7. Remote Ubuntu Deployment (Passwordless SSH Push)

After packaging locally, push the bundle to a remote Ubuntu host over
**SSH key-based authentication** and deploy there. Per-environment database
configuration lives centrally in the Ubuntu config root and is shared by all
instances.

```bash
# Repository root
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --environment production --replicas 3

# Reuse an existing bundle / explicit key
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.0.bundle \
    --ssh-key ~/.ssh/prod_ed25519 --environment test --replicas 2

# Other operations (forwarded to the remote deploy.sh)
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --environment test --down
```

From the sdkwork-space root (WSL / CI):

```bash
bash bin/remote-deploy-gateway.sh --host deploy@1.2.3.4 \
    --environment production --replicas 3
```

### 7.1 Prerequisites

- **Passwordless SSH**: run `ssh-copy-id deploy@1.2.3.4` once (defaults to
  `~/.ssh/id_ed25519`; use `--ssh-key` to override).
- **Remote sudo**: the default config root is
  `/etc/sdkwork/api-gateway/docker` (the standard config root,
  `APPLICATION_DEPLOY_LAYOUT_SPEC` §3), so the remote user needs passwordless
  sudo (`sudo -n`); otherwise pass `--config-root` pointing at a
  user-writable directory.

### 7.2 Config Root and Custom Database Password

After the push, the environment configuration lives on the remote Ubuntu:

```text
/etc/sdkwork/api-gateway/docker/<environment>.env   # single file, shared by all instances
```

- The remote file **wins**: re-deploys never overwrite it (unless `--sync-env`
  forces it from the local file).
- **Custom database password**: edit `GATEWAY_POSTGRES_PASSWORD`,
  `GATEWAY_POSTGRES_USER`, `GATEWAY_POSTGRES_DB`, ... in that file on the
  remote host, then re-run
  `deploy:apply:standalone:docker:remote --environment <env>`; all N
  instances of the environment read the same file automatically.
- First push generates it from the local `docker/env/<env>.env` (falling back
  to `.env.example`) when the remote file does not exist; mode `0600`.

### 7.3 Pipeline

1. Passwordless SSH probe (`BatchMode=yes`; on failure, hint `ssh-copy-id`);
2. Local packaging (or reuse `--bundle`);
3. Remote mkdir for the install root and config root;
4. Push/keep the remote env config;
5. `rsync -az` (falls back to `scp -r` when rsync is absent) pushes the bundle;
6. Remote `deploy.sh --environment <env> --replicas N --env-file <config-root>/<env>.env`;
7. Health-check instance 1's `/healthz`.

`--dry-run` previews every command; `--logs [N]` / `--ps` / `--down` forward
the action to the remote deploy.sh.

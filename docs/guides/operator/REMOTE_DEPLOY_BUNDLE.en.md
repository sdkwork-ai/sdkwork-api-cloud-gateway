# Bundle Remote Deploy Operations Guide (bundle → remote Ubuntu host)

> Version: 2026-09-04 · Applies to: `sdkwork-api-cloud-gateway` unified install image (bundle ≥ 0.1.1)
> Default remote host: `ubuntu@api.sdkwork.com` · Tool: `scripts/docker/remote-deploy.sh`
> Spec basis: `sdkwork-specs/DEPLOYMENT_SPEC.md` §6 · 中文版: [REMOTE_DEPLOY_BUNDLE.md](./REMOTE_DEPLOY_BUNDLE.md)

## 0. When to use this guide

Packaging is already done on your machine (see [docker-install.en.md](./docker-install.en.md)
§2; artifacts under `dist/docker-install/sdkwork-api-cloud-gateway-install-<version>.bundle/`).
Now push the bundle to a remote Ubuntu Docker host. **One command for the whole
pipeline**: local packaging (skippable) → SSH push → remote config materialization →
remote `deploy.sh` → health check.

The remote host needs exactly three things: **Ubuntu + Docker (with the compose
plugin)**, **passwordless sudo for the `ubuntu` user (`sudo -n`)**, and
**passwordless SSH from this machine**.

## 1. One-time prerequisites (do once, ~5 minutes)

On the local (build) machine:

```bash
# ① Generate an SSH key (skip if ~/.ssh/id_ed25519 already exists)
ssh-keygen -t ed25519

# ② Install the public key on the remote host (enter the ubuntu password once; passwordless afterwards)
ssh-copy-id ubuntu@api.sdkwork.com

# ③ Three-point check: passwordless SSH / passwordless sudo / Docker. All three must pass
ssh -o BatchMode=yes ubuntu@api.sdkwork.com \
  'echo ssh-ok; sudo -n true && echo sudo-ok; docker --version && docker compose version'
```

| Check | Fix when it fails |
| --- | --- |
| No `ssh-ok` | Re-run `ssh-copy-id ubuntu@api.sdkwork.com`; non-default key/port: `--ssh-key` / `--ssh-port` |
| No `sudo-ok` | Add passwordless sudo on the remote (`sudo visudo`, line `ubuntu ALL=(ALL) NOPASSWD:ALL`); or use `--config-root /home/ubuntu/sdkwork-config` (user-writable; the `/opt` install root still needs sudo, so passwordless sudo is recommended) |
| No `docker` | Install Docker on the remote: `curl -fsSL https://get.docker.com \| sudo sh` |

`rsync` on the local machine is optional (incremental transfers); without it the
script falls back to `scp` automatically.

## 2. One-command deploy (default host, copy & paste)

From the **repository root on the local machine**:

```bash
# development (single instance, external dependencies by default)
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment development

# test
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment test

# staging
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment staging

# production (3 instances; instance 1 runs migrations)
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --replicas 3
```

| Environment | Health-check port (instance 1) | Suggested node ID |
| --- | --- | --- |
| development | 3910 | 21 |
| test | 3911 | 22 |
| staging | 3912 | 23 |
| production | 3913 | 24 |

> `remote-deploy.sh` supports development / test / staging / production. For the
> demo environment, copy the bundle to the remote manually and run the bundle's
> `deploy.sh` directly (docker-install.en.md §3–§5).

The script performs 7 steps automatically:

1. **SSH probe**: verify passwordless login;
2. **Local packaging**: `package-install-bundle.mjs` builds/reuses the newest bundle (skip via §4);
3. **Push**: `rsync -az --delete` syncs the bundle to the remote `/opt/sdkwork/deploy/sdkwork-api-cloud-gateway/`;
4. **Config materialization**: local `docker/env/<env>.env` (falls back to `.example`) is pushed on first run to the remote `/etc/sdkwork/api-gateway/docker/<env>.env` (mode 0600);
5. **Remote deploy**: SSH runs the remote `deploy.sh --environment <env> --env-file <remote env> --replicas N`;
6. **Health check**: remote `curl /healthz`; failure exits non-zero;
7. Final line: `done: ubuntu@api.sdkwork.com <env> replicas=N`.

**Rehearse before going live** (prints the resolved commands only):

```bash
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com \
  --environment production --replicas 3 --dry-run
```

**Non-default SSH key / port**: `--ssh-key ~/.ssh/prod_ed25519 --ssh-port 2222`;
or pin them via the `SDKWORK_SSH_KEY` / `SDKWORK_SSH_PORT` environment variables.

## 3. Configuration after deploy (the only file to edit)

Remote config file: `/etc/sdkwork/api-gateway/docker/<env>.env` (mode 0600,
generated on first push). **This is the single env file read by every instance
of that environment**:

```bash
ssh ubuntu@api.sdkwork.com 'sudo nano /etc/sdkwork/api-gateway/docker/production.env'
```

- **Mandatory production items**: the 7 `<CHANGE_ME>` placeholders (one-liner
  generators in docker-install.en.md §4.1) + confirm
  `GATEWAY_POSTGRES_SSL_MODE=require`; self-check with
  `grep -n "<CHANGE_ME>"` (expect no output).
- **Making config changes take effect**: re-run the same §2 command (idempotent;
  the remote env file is not overwritten).
- **Re-runs never overwrite the remote config** — when the remote file exists,
  the remote copy wins (remote-side edits such as the database password are
  kept). To overwrite the remote file from local, pass `--sync-env` explicitly.
- **Multiple environments**: run the matching §2 command per environment on the
  same host; ports/networks/volumes/database names are isolated per environment,
  and node IDs are pre-wired to differ per the table above.

## 4. Reuse an existing bundle (skip local packaging)

When packaging is slow or the build machine is busy, point the script at an
existing bundle to skip step 2:

```bash
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com \
  --environment production --replicas 3 \
  --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.1.bundle
```

For offline hosts, verify integrity before/after the push (run on the remote):

```bash
ssh ubuntu@api.sdkwork.com \
  'cd /opt/sdkwork/deploy/sdkwork-api-cloud-gateway && sha256sum -c image.sha256'
# must print image.tar.gz: OK
```

## 5. Day-2 operations (same one command; the action is forwarded to the remote)

```bash
# status / follow logs (N = instance number)
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --ps
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --logs 1

# restart (returns only after healthy) / stop (keeps containers & volumes) / start
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --restart
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --stop
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --start

# remove (containers only; volumes kept) / remove + wipe volumes (embedded database data is unrecoverable!)
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --down
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --down --purge
```

Note: **remote env changes do NOT take effect via `--restart`** (restart only
restarts processes, it does not re-create containers) — re-run the §2 apply
once. Native commands (`docker ps --filter`, `docker compose -p ... ps`) are in
docker-install.en.md §5.1.

## 6. Upgrade and rollback

**Upgrade** (data lives in your external database or Docker volumes; upgrades
do not lose data):

```bash
# ① Package the new version locally (bump release.currentVersion in sdkwork.app.config.json, or --version)
bash scripts/build-install-bundle.sh
# ② Re-run the same deploy command: the new bundle is pushed, the remote env config is kept as-is
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --replicas 3
# ③ Verify
ssh ubuntu@api.sdkwork.com 'curl -fsS --noproxy "*" http://127.0.0.1:3913/healthz && curl -fsS --noproxy "*" http://127.0.0.1:3913/readyz'
```

The three pre-upgrade steps (env backup / keep the old bundle / database backup)
are in docker-install.en.md §5.3.

**Rollback** (either approach):

```bash
# Option A: push the previous-version bundle again (its image.tar.gz ships with the bundle, offline rollback works)
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com \
  --environment production --replicas 3 \
  --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.0.bundle

# Option B: when both images are already on the remote, just target the old tag (faster)
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com \
  --environment production --replicas 3 --image-tag 0.1.0
```

## 7. Troubleshooting quick table

| Symptom | Cause | Fix |
| --- | --- | --- |
| `passwordless SSH failed` | Passwordless not set up / wrong key or port | `ssh-copy-id ubuntu@api.sdkwork.com`; non-default: `--ssh-key` / `--ssh-port` |
| `remote mkdir failed (need passwordless sudo)` | `ubuntu` lacks passwordless sudo | Add `ubuntu ALL=(ALL) NOPASSWD:ALL` via `sudo visudo` on the remote, or `--config-root` to a user-writable directory |
| `healthz FAILED` | Database/Redis unreachable, missing secrets, TLS not configured | SSH in and read the logs: `ssh ubuntu@api.sdkwork.com 'docker logs --tail 200 sdkwork-api-cloud-gateway-<env>-i1-gateway-1'`, then follow docker-install.en.md §9.1 |
| Remote edits silently reverted | The deploy command included `--sync-env` | `--sync-env` is an explicit overwrite; do not use it routinely — existing remote files are never overwritten by default |
| rsync slow on many small files | Normal on first push; later runs transfer deltas only | Nothing to do; or confirm `rsync` is installed locally (avoids full scp) |
| Port already allocated | Leftover old fleet | On the remote: `docker ps --format '{{.Names}}\t{{.Ports}}'` |

The full symptom table (restart loops / migration waits / proxy poisoning etc.)
is in [docker-install.en.md](./docker-install.en.md) §9; Day-2 and file-location
quick reference in §5.1–§5.2.

---

## Appendix: local ↔ remote resource map

| Item | Local (build/packaging machine) | Remote `ubuntu@api.sdkwork.com` |
| --- | --- | --- |
| Install bundle | `dist/docker-install/...bundle/` | `/opt/sdkwork/deploy/sdkwork-api-cloud-gateway/` |
| Environment config | `docker/env/<env>.env` (first-push source) | `/etc/sdkwork/api-gateway/docker/<env>.env` (0600, the only runtime-effective config) |
| Deploy entrypoint | `scripts/docker/remote-deploy.sh` | bundle `deploy.sh` (invoked by the script) |
| Docker resources | packaging only (image build) | network & volumes `sdkwork-api-cloud-gateway-<env>-*`, containers & logs (§5.2 quick reference) |

# Release Lifecycle Operator Guide (English)

> Audience: operators/developers who need to **release, upgrade, roll back, and inspect** the
> Docker deployment of sdkwork-api-cloud-gateway.
> Prerequisite: [Docker Foolproof Install Guide](docker-install.en.md) — that guide covers the
> first install; this guide covers day-2 releases.
> Single command channel: the `bin/` nine-entrypoint family (`MODULE_BIN_SPEC.md`). Every
> release action (including rolling apply on the target host, health gating, and ledger
> bookkeeping) is driven by `bin/docker-deploy.sh`; the in-bundle executors are internal
> implementation details of `bin/` — operators never call scripts by hand.

## TL;DR

Daily release is two commands:

```bash
bin/docker-image.sh build                                          # build the canonical image (tag from agent.app.config.json)
bin/docker-deploy.sh upgrade --environment production --image-tag <version>   # upgrade (health gate + auto-rollback + ledger)
```

When something breaks, one more:

```bash
bin/docker-deploy.sh rollback --environment production             # back to the previous successful version in seconds
```

Append `--host ssh://[user@]host[:port]` for remote targets; `--dry-run` prints the plan
without side effects; production requires explicit `--yes`.

## 1. Command quick reference (all via bin/)

| Action | Command | Risk |
| --- | --- | --- |
| Install / first release | `bin/docker-deploy.sh install --environment <env>` (add `--yes` in production) | changes the running version |
| Upgrade to a version | `bin/docker-deploy.sh upgrade --environment <env> --image-tag <version>` | changes the running version |
| Rollback | `bin/docker-deploy.sh rollback --environment <env> [--to <version>]` | changes the running version |
| Status / drift detection | `bin/docker-deploy.sh status --environment <env>` | read-only |
| Health probe | `bin/doctor.sh --environment <env>` | read-only |
| Logs | `bin/docker-deploy.sh logs --environment <env> [--tail N] [--follow]` | read-only |
| Stop / start / restart | `bin/docker-deploy.sh stop/start/restart --environment <env>` | ops action |
| Retire | `bin/docker-deploy.sh down --environment <env> [--purge --yes]` | removes the stack |

## 2. Built-in safety mechanisms (always on, zero config)

- **Release mutex**: release/rollback per environment take a lock; concurrent releases fail
  fast instead of interleaving; clear a stale lock only after confirming nothing is running.
- **Bundle integrity check**: when the local image is missing and must be loaded from the
  bundle's `image.tar.gz`, the sha256 sidecar is verified first (corrupted/tampered bundles
  are rejected); the target tag is also checked against the tag the bundle carries.
- **Health gate**: rolling apply per instance (instance 1 migrates and turns healthy before
  instances 2..N start), each instance probed on `/healthz` (180s budget).
- **Release ledger**: append-only record of every release/rollback (who released what when,
  with what result); a failed upgrade auto-rolls back to the pre-change version and records
  `failed`.

## 3. Standard release flow (production to 0.2.0)

```bash
# 0. (optional) dry-run: print the plan only
bin/docker-deploy.sh upgrade --environment production --image-tag 0.2.0 --dry-run

# 1. Confirm the current state
bin/docker-deploy.sh status --environment production

# 2. Build and release (bin builds and syncs the image; point --host at the target)
bin/docker-image.sh build
bin/docker-deploy.sh upgrade --environment production --image-tag 0.2.0 --yes

# 3. Post-release verification
bin/docker-deploy.sh status --environment production
bin/doctor.sh --environment production
curl --noproxy '*' http://127.0.0.1:3913/healthz   # production base port (+10 per instance)
```

## 4. Rollback

```bash
bin/docker-deploy.sh rollback --environment production             # previous successful version (most common)
bin/docker-deploy.sh rollback --environment production --to 0.1.1  # explicit version (image must exist locally or in a bundle)
bin/docker-deploy.sh status     --environment production
```

- Rollback passes the same health gate; failures are ledgered and reported immediately.
- Rolled-away versions stay in the ledger and can be re-entered later via `--to <version>`
  (roll-forward).
- **⚠️ Database migrations do not roll back**: rollback swaps the image only; applied
  migrations stay. Every migration must therefore be backward compatible
  (expand-then-contract). The same constraint applies in Helm/Argo worlds.
- **Cross-bundle rollback**: if the new bundle's compose template changed destructively,
  `bin/docker-deploy.sh rollback` refuses; fall back with
  `bin/docker-deploy.sh upgrade --image-tag <old-version>` (image and runtime config both
  return to the old version). **Keep the last 2–3 image versions on the host.**

## 5. Status and ledger

```bash
bin/docker-deploy.sh status --environment production
```

Example output:

```
environment production
  deployed version : 0.2.0 (at 2026-09-05T02:00:00Z)
  recorded image   : registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.2.0 sha256:abcd...
  topology         : 1 instance(s), host ports 3913 (+10 per instance) -> container 8080, deps=external
  instance 1 : port 3913 HEALTHY
  drift            : none (containers match the recorded release)
```

A non-empty `drift` means someone mutated containers bypassing `bin/` (e.g. a manual
`docker compose up`) — investigate.

Ledger files (append-only, archivable):

```
<bundle>/release-state/<environment>/ledger.jsonl   # one JSON record per line
<bundle>/release-state/<environment>/current.json   # current-version snapshot
```

Custom ledger location: `export SDKWORK_RELEASE_STATE_DIR=/var/lib/sdkwork-gateway-release`
(the external `--env-file` mode defaults to `release-state/` next to the env file, e.g.
`/etc/sdkwork/api-gateway/docker/release-state/`).

## 6. How the daily dev flow plugs in

| Step | How |
| --- | --- |
| Local dev testing | `pnpm dev` (local process, outside the release system) |
| Build a new version | On the build host: `bin/docker-image.sh build` (image + build evidence) |
| Ship to a target host | `bin/docker-deploy.sh install/upgrade --environment <env> --host ssh://ops@<host>` (bundle sync, image load, health gate in one command) |
| Test first | `bin/docker-deploy.sh upgrade --environment test --image-tag <version>` + `status` |
| Staging / production | Same, environment by environment; leave a `status` trace each step; add `--yes` in production |
| When it breaks | `bin/docker-deploy.sh rollback --environment <env>` (back in seconds) |
| Stop / start / restart | `bin/docker-deploy.sh stop/start/restart --environment <env>` |

## 7. FAQ

- **Upgrade fails with "image ... not found locally"**: the target image is not on the
  target host. `bin/docker-image.sh save -o dist/image.tar.gz` then
  `bin/docker-image.sh load -i image.tar.gz` there, or let
  `bin/docker-deploy.sh install --host ...` handle bundle sync and image loading.
- **Upgrade fails with "another release operation holds ... .lock"**: a release is already
  running for this environment. Wait for it; clear a stale lock only after confirming
  nothing is running.
- **Upgrade fails with "this bundle carries ... not ..."**: the `--image-tag` version does
  not match the image the bundle carries. Run `bin/docker-image.sh build` for the target
  version first, then retry.
- **Rollback fails with "no previous successful version"**: nothing was ever released to
  this environment (empty ledger). Establish a baseline with `install/upgrade` first.
- **Upgrade failed, auto-rolled back, but I want the root cause**:
  `bin/docker-deploy.sh logs --environment <env> --tail 200` for instance logs,
  `bin/docker-deploy.sh status` for the ledger result. If the failed version equals the
  current version (idempotent re-release failure), auto-rollback explicitly skips; fix the
  root cause and re-release.
- **Scaling**: `bin/docker-deploy.sh install --environment <env> --replicas 3`.
- **Canary limitations**: single-host docker compose topology; upgrades roll per instance
  (instance 1 healthy before 2..N start). Proportional traffic shifting (Argo Rollouts
  style) is out of scope; validate fully on test/staging before multi-instance production.

## 8. Relation to the specs

- Operator channel contract: `MODULE_BIN_SPEC.md` §4.1/§4.2; bundle contract:
  `DOCKER_SPEC.md` §4 and `OPERATIONS_SPEC.md` §1.2.
- The release ledger mechanism does not change the compose topology, port plan, or health
  check contract (`GATEWAY_CONTAINER_PORT` attach contract unchanged).
- The ledger is runtime state, never committed (`.gitignore` excludes `release-state/`).

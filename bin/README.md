# bin/ — standardized entrypoints (`sdkwork-specs/MODULE_BIN_SPEC.md`)

`sdkwork-api-cloud-gateway` is the API gateway: it `MUST NOT` publish to the
public internet directly and is reached only through `sdkwork-webserver`
(`DOCKER_SPEC.md` §1). Shared behavior lives in
`sdkwork-specs/bin/lib/sdkwork-common.sh`; this directory only carries
identity and delegation.

| Script | Purpose |
| --- | --- |
| `docker-image.sh` | build / push / save / load / update / inspect `registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:<version>` |
| `docker-deploy.sh` | install / upgrade / rollback / status / logs / down / start / stop / restart the Docker bundle on `wsl` or `ssh://[user@]host` |
| `apps-build.sh` | build `server` (cargo release) |
| `apps-package.sh` | package the server artifact into `target/bin-packages/` (+ sidecar `.sha256`) |
| `apps-deploy.sh` | install the server package on WSL Ubuntu / remote Ubuntu |

Declared app types: `server`. Default image tag comes from
`agent.app.config.json` → `release.currentVersion`.

## Sub-layout (`MODULE_BIN_SPEC.md` §2.1)

Only the nine entrypoints above are the operator surface. Everything else under
`bin/` is library, asset, or build input — invoked by the entrypoints, never by
hand:

| Path | Contents |
| --- | --- |
| `lib/` | `bootstrap.sh` + `module.sh` (module identity, image name, app-type map, delegation commands) |
| `container/` | scripts baked into the image: `entrypoint` (container `ENTRYPOINT`), `knowledgebase-rpc-entrypoint`, `postgres/{entrypoint.sh,init/}` (embedded-mode assets) |
| `host/` | docker / WSL host preparation: `prepare-host.sh`, `provision-external-databases.sh`, `external-schema.sql`, `wsl-reset-db-passwords.sh`, `bind-windows-hosts.ps1` |
| `packaging/` | package-builder inputs: `install-ubuntu.sh` (shipped as `install/install-ubuntu.sh`), `build-install-bundle.sh`, `validate-ubuntu-package.sh` |
| `docker-bundle-*.sh` | target-side bundle executors, copied to the bundle root as `deploy.sh` / `release.sh` / `prepare-envs.sh` |

The build outputs the artifact copies: `Dockerfile` stages `bin/container/*`
into the image's `container/` directory, and the packagers copy
`bin/docker-bundle-*.sh` and `bin/container/postgres/*` into the bundle.

## Container path (every environment) — canonical

```sh
bin/docker-image.sh build                                # → build:container --tag registry.sdkwork.com/apps/…:<version>
bin/docker-image.sh save -o dist/sdkwork-api-cloud-gateway.tar.gz
bin/docker-deploy.sh install --environment demo          # wsl target
bin/docker-deploy.sh install --environment production --yes --host ssh://ops@10.0.0.9
bin/docker-deploy.sh status  --environment demo
bin/docker-deploy.sh logs    --environment demo
bin/docker-deploy.sh rollback --environment demo --to 0.1.0
bin/docker-deploy.sh down    --environment demo --purge --yes
```

The bundle is synced to `/opt/deploy/sdkwork-api-cloud-gateway/bundle` on the
target and `deploy.sh` is executed with that directory as the working
directory. This bundle also ships `release.sh`, so `rollback` uses
bundle-owned release history (`--to <version>` for an explicit target).

### Five environments — copy-paste ready

| Environment | Host port | Profile id | Domain family | Database |
| --- | --- | --- | --- | --- |
| `development` | `3910` | `standalone.development` | `api-dev.*` | `sdkwork_ai_dev` |
| `test` | `3911` | `standalone.test` | `api-test.*` | `sdkwork_ai_test` |
| `staging` | `3912` | `standalone.staging` | `api-staging.*` | `sdkwork_ai_staging` |
| `demo` | `3914` | `standalone.demo` | `api-demo.*` | `sdkwork_ai_demo` |
| `production` | `3913` | `standalone.production` | `api.*` | `sdkwork_ai_prod` |

```sh
bin/docker-deploy.sh install --environment development            # 3910
bin/docker-deploy.sh install --environment test                   # 3911
bin/docker-deploy.sh install --environment staging                # 3912
bin/docker-deploy.sh install --environment demo                   # 3914
bin/docker-deploy.sh install --environment production --yes       # 3913 (--yes required)
```

> Per-environment command blocks (upgrade / rollback / status / logs / down /
> verification) are in
> [`docs/guides/operator/docker-install.md`](../docs/guides/operator/docker-install.md) §4.

## Host-native path (production only)

```sh
bin/apps-build.sh   server production
bin/apps-package.sh server production:standalone --out target/bin-packages
bin/apps-deploy.sh  server install production --yes --host ssh://root@10.0.0.9
bin/apps-deploy.sh  server status  production
```

The server tarball ships a single production config template
(`RELEASE_CONFIG_TEMPLATES` in `package-server.mjs`), so the host-native
channel covers **production only**. Every other environment uses the
container path above; the entrypoint fails fast with that guidance.

`apps-deploy.sh server install` pushes the archive to
`/opt/deploy/sdkwork-api-cloud-gateway/packages`, extracts it, runs
`install/install-ubuntu.sh`, then enables and health-checks
`sdkwork-api-gateway.service`. The remote account needs root privileges; no
`sudo` is injected. Host-native `rollback` is not tracked by the channel —
package the previous version and install it instead.

## Shared flags

`--environment development|test|staging|demo|production` ·
`--profile standalone|cloud` (as `<environment>:<profile>` for apps-\*) ·
`--host wsl|ssh://[user@]host[:port]` · `--image-tag <v>` ·
`--deps external|embedded` · `--to <version>` · `--purge` · `--yes` ·
`--dry-run` · `-h|--help`

`--dry-run` prints the plan and executes nothing. Production mutations require
`--yes`; `--purge` requires `--yes` in every environment. Each run appends a
line with its exit status to `target/bin-evidence/evidence.log`.

Run `bin/<script>.sh doctor` for the environment self-check.

## Operations lifecycle

```bash
bin/config.sh  <list|show|get|set|diff|validate|edit> --environment <env> [--key K] [--value V] [--reveal]
bin/doctor.sh  --environment <env> [--instance N] [--json] [--export <dir>]
bin/backup.sh  <create|list|verify|restore> --environment <env> [--set <name>] [--component all|config|database|volumes]
bin/docker-deploy.sh logs --environment <env> [--instance N] [--service <s>] [--tail N|all] [--since <d>] [--follow] [--export <dir>]
```

`config.sh` reads the live bundle configuration on the target, redacts secrets,
and backs up before mutating. `doctor.sh` is read-only and exits 70 when a
check fails. `backup.sh` writes checksummed sets to
`/opt/deploy/<module>/backups/` on the target. Runbooks: `docs/runbooks/`.

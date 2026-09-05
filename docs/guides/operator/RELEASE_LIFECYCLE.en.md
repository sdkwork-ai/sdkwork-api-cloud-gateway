# Release Lifecycle Operator Guide (English)

> Audience: operators/developers who need to **release, upgrade, roll back, and inspect** the
> Docker deployment of sdkwork-api-cloud-gateway.
> Prerequisite: [Docker Foolproof Install Guide](docker-install.en.md) — that guide covers the
> first install; this guide covers day-2 releases.

## TL;DR

Daily release is one command:

```bash
bash release.sh deploy --environment <env> --image-tag <version>
```

It ships with: pre-upgrade snapshot → rolling apply → HTTP health gate →
**automatic rollback on failure** → version ledger. When something breaks:

```bash
bash release.sh rollback --environment <env>
```

## 1. Where release.sh lives

| Scenario | Location |
| --- | --- |
| Install-bundle deployment | Bundle root, next to `deploy.sh`: `<bundle>/release.sh` |
| Repository source deployment | `scripts/docker/bundle/release.sh` (or `pnpm release:deploy:standalone:docker -- --environment dev`) |

> Requirements: `release.sh` must sit next to `deploy.sh`; bundle deployments also need
> `compose/`, `env/`, and `image.env` (the packager guarantees this).

## 2. The six subcommands

| Command | Purpose | Mutates? |
| --- | --- | --- |
| `deploy` | First release / upgrade to a target version (health gate + auto rollback) | yes |
| `rollback` | Go back to the previous successful version (or `--to <version>`) | yes |
| `status` | Deployed version, actual container image (tag + image ID double check), per-instance health, drift check | read-only |
| `history` | Release ledger (who released what, when, with what result) | read-only |
| `verify` | Health probe only | read-only |
| `versions` | List locally available image versions (deploy/rollback candidates) | read-only |

Common options: `--environment development|test|staging|demo|production` (required),
`--replicas N`, `--embedded`/`--external`, `--env-file <abs-path>`,
`--dry-run` (print the plan without executing).

Safety mechanisms (on by default):

- **Release lock**: `deploy`/`rollback` take a per-environment lock (atomic
  `.lock` directory creation); concurrent releases fail loudly instead of
  interleaving. Wait budget: `--lock-timeout <seconds>` (default 120). Remove a
  stale `.lock` directory only after confirming no release is running.
- **Bundle integrity check**: when the image must be loaded from
  `image.tar.gz`, the `image.sha256` checksum is verified first (corrupted or
  tampered bundles are rejected); bypass with `--skip-verify`. The target tag
  is also checked against the tag the bundle actually carries, avoiding the
  confusing "loaded A, asked to deploy B" failure mode.

## 3. Standard release flow (upgrade)

Upgrading production to 0.2.0:

```bash
# 0. (optional) rehearse: print the plan only
bash release.sh deploy --environment production --image-tag 0.2.0 --dry-run

# 1. confirm the current state
bash release.sh status --environment production

# 2. release (the target image must exist locally, or come from the bundle image.tar.gz)
bash release.sh deploy --environment production --image-tag 0.2.0

# 3. post-release verification
bash release.sh status --environment production
bash release.sh verify --environment production
curl http://127.0.0.1:3913/healthz   # production base port (each instance +10)
```

What `deploy` does, in order:

1. Reads the deployed version from the ledger (`-` on first release);
2. Calls the co-located `deploy.sh --image-tag <version>` for a rolling apply
   (instance 1 migrates and must turn healthy before instances 2..N start);
3. Probes `/healthz` on every instance port (budget 180s by default, tune with
   `--health-timeout`);
4. All green → writes `current.json` and appends a `success` ledger record;
5. Any failure → appends a `failed` record and **automatically rolls back to the
   pre-upgrade version** (disable with `--no-auto-rollback`).

## 4. Rollback

```bash
# back to the previous successful version (most common)
bash release.sh rollback --environment production

# back to an explicit version (its image must exist locally or be loadable from a bundle; see `versions`)
bash release.sh rollback --environment production --to 0.1.1
```

- Rollback passes the same health gate; a failed rollback is recorded and reported loudly.
- The rolled-away version stays in the ledger; you can roll forward again later with `--to`.
- **⚠️ Database migrations are NOT rolled back**: rollback swaps the image (code) only;
  applied migrations stay. Every release must ship backward-compatible migrations
  (expand/contract) so the previous version remains runnable on the new schema. This is the
  industry-standard constraint (Helm/Argo behave the same).
- **Cross-bundle rollback**: if a newer bundle changed the compose templates in a breaking
  way, run the rollback from the older bundle directory's `release.sh`/`deploy.sh` instead
  (both image tag and templates revert). Keep the last 2–3 bundle directories on the host.

## 5. Status and ledger

```bash
bash release.sh status --environment production
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

A non-empty `drift` means someone mutated containers outside release.sh (e.g. a manual
`docker compose up`); investigate.

Ledger files (append-only, safe to archive):

```
<bundle>/release-state/<environment>/ledger.jsonl   # one JSON record per line
<bundle>/release-state/<environment>/current.json   # current version snapshot
```

Custom location: `export SDKWORK_RELEASE_STATE_DIR=/var/lib/sdkwork-gateway-release`.
With the external `--env-file` mode the ledger defaults next to the shared env file,
e.g. `/etc/sdkwork/api-gateway/docker/release-state/`.

## 6. How it fits the daily development flow

| Step | How |
| --- | --- |
| Local dev | `pnpm dev` (local process, outside the release system) |
| Build a new version | On the build machine: `bash scripts/build-install-bundle.sh --version 0.2.0` (output: `dist/docker-install/...-0.2.0.bundle`) |
| Ship to the target host | `scp -r`, or reuse `scripts/docker/remote-deploy.sh`; keep old bundle directories |
| Test environment first | `bash release.sh deploy --environment test --image-tag 0.2.0` + `verify` |
| Staging / production | Same, one environment at a time; `status` after each step |
| Something breaks | `bash release.sh rollback --environment <env>` (back to the previous version in seconds) |
| Stop / start / restart | Still `deploy.sh --stop / --start / --restart` (ops actions, not releases) |

## 7. FAQ

- **`deploy` says "image ... not found locally"**: the target image is not on the host.
  Load it from the matching bundle (`docker load -i image.tar.gz`) or produce/ship it first;
  `release.sh versions --environment <env>` lists local candidates.
- **`deploy` says "another release operation holds ... .lock"**: a release is in flight for
  this environment. Wait for it; remove a stale `.lock` directory only after confirming
  nothing is running.
- **`deploy` says "this bundle carries ... not ..."**: the requested `--image-tag` differs
  from the image inside this bundle. Run from the bundle of the target version, or
  build/pull the target image first.
- **`rollback` says "no previous successful version"**: this environment never recorded a
  release (empty ledger). Establish the baseline with `deploy` first.
- **A failed deploy auto-rolled back; where are the failure details?**
  `bash deploy.sh --environment <env> --logs 1` for instance logs and
  `bash release.sh history --environment <env>` for the failed record. If the failed
  version equals the previously deployed one (failed idempotent re-apply), auto-rollback
  is deliberately skipped as a no-op — fix the root cause and re-run deploy.
- **Scaling replicas**: `deploy.sh --environment <env> --replicas 3` directly;
  `release.sh deploy` forwards `--replicas` too.
- **Health-gate budget too short/long**: `--health-timeout <seconds>` (default 180).
- **Canary/gradual-rollout limitation**: the current topology is single-host docker
  compose; `deploy` rolls instances sequentially (instance 1 must be healthy before
  2..N start), which gives basic rolling protection. Percentage-based canary traffic
  splitting (à la Argo Rollouts) needs gateway-level support and is out of scope. For
  multi-instance production, validate fully on test/staging first.

## 8. Spec alignment

- Packaging chain: `DEPLOYMENT_SPEC.md` §6 / `PNPM_SCRIPT_SPEC.md` §4.4 (unchanged).
- `release.sh` is a lifecycle wrapper around `deploy.sh`; it does not alter the compose
  topology, port plan, or the `GATEWAY_CONTAINER_PORT` attach contract.
- The ledger is runtime state — never committed (`.gitignore` excludes `release-state/`).

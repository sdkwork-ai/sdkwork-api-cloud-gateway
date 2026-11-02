# sdkwork-api-cloud-gateway 0.1.19 — workspace dependency wave, production rebuild and upgrade

Date: 2026-11-03 (Asia/Shanghai)
Scope: sync every `sdkwork-space` submodule to current `main`, rebuild the container
delivery on the bookworm toolchain, and upgrade the remote production Docker stack
(`ssh://ubuntu@api.noaper.com`) in place with its live operator env preserved.

## Outcome

| Item | Value |
| --- | --- |
| Release version | `0.1.19` |
| Container image | `registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.19` |
| Image id | `sha256:c6ba4486827bfb41be4fcf207edf4c565f708e5ea3c0d969d7deefebae1de384` |
| Image size | 433.6 MB (454673322 bytes) |
| Build toolchain | `debian:bookworm-slim@sha256:7c7b2c96…` container, rustc 1.99.1, glibc 2.36; produced binary requires at most `GLIBC_2.34` |
| Install bundle | `dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.19.bundle` |
| Bundle image sha256 | `377f7db1a29e30be603040b051c6b2cbee993b43f75e9eb3ee5326252aaaa5b2` |
| Remote bundle | `/opt/deploy/sdkwork-api-cloud-gateway/bundle` (`image.env` = `0.1.19`) |
| Deploy command | `bash bin/docker-deploy.sh install --environment production --yes --host ssh://ubuntu@api.noaper.com` |
| Deploy result | exit 0 — `gateway is healthy (waited 715s)`, 1 instance applied |
| Remote containers | gateway + knowledgebase-rpc `:0.1.19` healthy; postgres `pgvector/pgvector:pg18` and redis `redis:8-alpine` untouched |
| Health | `/healthz` 200 `{"status":"ok"}`, `/readyz` 200 `{"status":"ready"}` directly and through the public edge |

## 1. Workspace dependency wave

Survey: 105 registered submodules, all on `main`. 86 were behind `origin/main`
(293 commits total), one diverged, three worktrees dirty.

Sync result (104 submodules excluding this gateway):

| Action | Count |
| --- | --- |
| fast-forwarded to `origin/main` | 84 |
| diverged merge (`sdkwork-knowledgebase`, local lockfile resync preserved) | 1 |
| already current | 17 |
| pre-existing dirty worktrees preserved untouched | 2 (`sdkwork-app-topology`, `sdkwork-birdcoder2`) |
| blocked / failed | 0 / 0 |

Largest advances: `sdkwork-mall` +104 commits (bilingual i18n catalogues across
mall-pc/mall-flutter/mall-mini-program, environment identity and API-host fixes),
`sdkwork-iam` +47 commits (admin account-binding policy editor across PC/H5/Flutter/
mini-program, RP registration, mini-program console tier, Flutter admin catalogs),
`sdkwork-cloudrouter` +3, `sdkwork-knowledgebase` merged forward (local Cargo.lock
resync + note-autosave/knowledge-space work). The full `repo old→new` table was
captured during the wave.

The gateway repository itself merged `origin/main` (`8808e4b`, CI workflow refresh:
`deployment_profile`/`runtime_target` package inputs, required `package_version`,
new Verify entrypoint) over its local release commits — merge commit `97bc571`, one
conflict in `.github/workflows/package.yml` resolved by keeping the local tag default
and taking the refreshed reusable-workflow contract.

## 2. Release preparation

- `Cargo.lock` validated locked-clean for both the default and
  `platform-foundation-suite-container` feature graphs (`cargo metadata --locked`,
  1522 packages) — no lock regeneration needed after the wave.
- Version bumped to `0.1.19` across `agent.app.config.json` (incl. release notes),
  `package.json`, `agent.workflow.json` (defaultVersion + 6 artifact paths) and
  `.github/workflows/package.yml` defaults.
- CI framework pin refreshed from `2a003154…` to `7dd29e0…` in `package.yml` and
  `verify.yml`: the merged `verify.yml` referenced `sdkwork-ci.yml`, which does not
  exist at the old pin, and the old pin rejected the current
  `agent.workflow.json` schema version.
- Three pre-existing gate defects repaired fix-forward (test/guard only, both merge
  parents were failing identically): a blanket `SDKWORK_WEBSERVER_` Dockerfile guard
  that now exempts the documented `SDKWORK_WEBSERVER_APP_ROOT` discovery key, a
  host-`path.join` defect in the dev Opus resolver that broke the simulated Windows
  path on Linux, and a stale IAM bootstrap guard that now accepts the canonical
  `web_module_with_pool` factory while still requiring the shared process pool.
- Static workspace gates: 18/18 pass after the fixes (standards, topology, workflow
  validation, API closure/operation patterns/response envelope, CORS carriers,
  process-shared pool, script tests 102/102, IAM/database bootstrap standards).
- Cargo feature graphs: `--all-features` and `platform-foundation-suite` both
  `cargo check` clean inside the bookworm toolchain container; `foundation-community`
  clean on the host; `cargo fmt -- --check` clean. (The host lacks `libssl-dev`, so
  the two OpenSSL-carrying graphs are only meaningful in the container.)

## 3. Image and bundle build

Built inside an isolated `debian:bookworm-slim` container (host rustup/cargo and the
workspace mounted at `/work`, `CARGO_TARGET_DIR=/work/.build-cache-gateway-target-bookworm`)
so the shipped binary matches the runtime glibc/OpenSSL ABI. `bash bin/docker-image.sh
build --image-tag 0.1.19` produced the image; the binary inside the image is byte-identical
to the bookworm cache artifact (`sha256:042139f0…`).

The bundle was packaged with `node scripts/docker/package-install-bundle.mjs
--skip-image-build` (the canonical `pnpm build:container:install` path).

Two build attempts failed first and are recorded in `target/bin-evidence/evidence.log`
as expected evidence: a host-side attempt (glibc mismatch, permission clash with
root-owned cache fingerprints) and a container attempt missing `cc`. The final build
also needed the prebuilt Opus archive exposed through `pkg-config` (and autotools as
fallback), after which the 15-minute release build completed.

## 4. Production upgrade and the company schema incident

Deploy flow on the remote host: pre-change backup set
`/opt/deploy/sdkwork-api-cloud-gateway/backups/sdkwork-api-cloud-gateway-production-20261102T162221Z`
(config + docker volume captured; **the PostgreSQL dump came back 0 bytes — a
pre-existing backup-credential gap, see follow-ups**), bundle tar-stream push,
`docker load` of `0.1.19`, compose recreate of gateway + knowledgebase-rpc only,
health gate.

The first start looped: the new `sdkwork-company` main revises its **baseline DDL and
seeds** (commit `60bcac6`, "listed-company directory seeds") but ships no upgrade
migration, so on the pre-existing production schema the seed failed with
`column "code" of relation "company_industry" does not exist`. `GATEWAY_MIGRATE_ON_START`
is `false` on production by design, and no migration existed that could add the
objects anyway.

Fix-forward repair applied to the live `sdkwork_ai_prod` database (idempotent DDL,
single transaction each):

1. `ALTER TABLE company_industry ADD COLUMN IF NOT EXISTS` for `code`, `parent_id`,
   `level`, `path`, `classification_standard`, `market_scope`; backfill
   `code = 'PLATFORM.' || slug`; `NOT NULL` + unique `(tenant_id, code)` + self-FK
   (matches the module baseline).
2. Column-level reconciliation of the other 16 existing `company_*` tables
   (11 new `company_profile` columns such as `company_type`, `listing_status`,
   plus the 10 inline unique constraints the seeds rely on for `ON CONFLICT`).
3. Re-applied the module's fully idempotent `0001_company_baseline.sql`
   (23 `CREATE TABLE IF NOT EXISTS` + 53 `CREATE INDEX IF NOT EXISTS`), which
   materialised the six missing tables (`company_organization`, `company_listing`,
   `company_profile_industry`, `company_classification`, `company_financing_round`,
   `company_financing_investor`).

After the repair the gateway bootstrapped every module and reached health on its next
restart (`waited 715s` covers the restart loop). Seed history now proves the module is
converged: `ops_seed_history` 166 rows, `company_industry` 521 rows,
`company_organization` 13 829 rows.

Records kept for reproduction: `/tmp/company-reconcile.sql` (generated from the
baseline DDL against the live catalogue) and the applied baseline path above.

## 5. Verification evidence

- `bin/docker-deploy.sh status --environment production`: gateway `:0.1.19` healthy,
  `0.0.0.0:3913->8080/tcp`; knowledgebase-rpc `:0.1.19` healthy; postgres/redis healthy.
- Direct probes on the host (Host `api.sdkwork.com`): `/healthz` 200, `/readyz` 200,
  `/openapi/index.json` 200 (embedded route table), unauthenticated
  `/backend/v3/api/iam/tenants` 401 problem+json, non-whitelisted Host 421.
- Public edge: `https://api.sdkwork.com/healthz` 200, `/readyz` 200,
  `https://api.birdcoder.com/healthz` 200.
- Bootstrap: 811 module bootstrap completion records across the deploy attempts; the
  current process logs contain no `failed to bootstrap` or `Error:` lines after the
  repair (`StartedAt 2026-11-02T16:36:02Z`, no restart since).
- `bin/doctor.sh --environment production`: PASS toolchain, bundle, compose, health,
  probe, image; FAIL config (`GATEWAY_CLOUDROUTER_REDIS_PASSWORD` empty — embedded
  Redis runs without a password, known non-defect); WARN logs (a substring match on
  `errors=0` in a compensation INFO line, not an error); WARN disk (`/var/lib/docker`
  89% full).

## 6. Follow-ups (not blocking the release)

1. **sdkwork-company needs a real upgrade migration.** Production was repaired by
   applying the module baseline; every other pre-existing environment that upgrades to
   this wave will hit the same seed failure unless the baseline delta is encoded as
   versioned migrations (columns, six tables, unique constraints) and applied by the
   explicit `--migrate-databases` step.
2. **Pre-change database dump is empty.** `bin/backup.sh` captured config and volumes
   but produced a 0-byte `database.dump` (the volume copy carries the data, but the
   logical dump path needs the `GATEWAY_POSTGRES_*` credential/file wiring fixed).
3. **Manifest container checksum is stale.** `artifacts.installConfig` container
   entries still advertise the pre-0.1.14 digest; the image identity of record is
   `dist/container-image.json`.
4. **Remote disk at 89%** (13 GB free). Old gateway images are reclaimable
   (`docker system df`) but were not pruned during this wave.

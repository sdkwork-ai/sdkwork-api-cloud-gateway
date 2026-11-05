# sdkwork-api-cloud-gateway 0.1.27 — workspace wave, IAM organization migration fix-forward and production upgrade

Date: 2026-11-05 (Asia/Shanghai)
Scope: sync every `sdkwork-space` submodule to current `main`, rebuild the container delivery on the
bookworm toolchain, upgrade the remote production Docker stack (`ssh://ubuntu@api.noaper.com`) in
place with its live operator env preserved, and apply the IAM migrations the new wave requires.

## Outcome

| Item | Value |
| --- | --- |
| Release version | `0.1.27` |
| Container image | `registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.27` |
| Image id | `sha256:dddfce7e3ca76d52dc0cb0c8e50a8b9ffcacb687550d199cd55f1cc382d9bc7d` |
| Image size | 434.5 MB (455,633,787 bytes) |
| Build toolchain | `debian:bookworm-slim` container `gw-build-023`, rustc 1.99.1, glibc 2.36, static Opus 1.6.1 |
| Build window | 2026-11-05T12:19:35Z → 12:41:42Z (22 m 07 s) |
| Install bundle | `dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.27.bundle` |
| Bundle image sha256 | `2ea0808ec3ebf5fcdf5b9c7b89219deb126a7c829def1cc3e618f745caba66e2` (453,930,395 B) |
| Remote bundle | `/opt/deploy/sdkwork-api-cloud-gateway/bundle` (`image.env` = `0.1.27`) |
| Deploy command | `bash bin/docker-deploy.sh upgrade --environment production --deps embedded --host ssh://ubuntu@api.noaper.com --yes` |
| Deploy result | exit 0 — `gateway is healthy (waited 35s)`, 1 instance applied, RestartCount 0 |
| Remote containers | gateway + knowledgebase-rpc `:0.1.27` healthy; postgres `pgvector/pgvector:pg18` (4 d) and redis `redis:8-alpine` (6 d) untouched |
| Health | `/healthz` 200 `{"status":"ok"}` (15 B), `/readyz` 200 `{"status":"ready"}` (18 B), direct and through the nginx edge |
| API surface | `/openapi/index.json` 200, 1,219,789 B, 3,424 route entries (+2,092 B over 0.1.25) |
| Database | 182 migration history rows (was 178): IAM `0024`–`0027` applied |
| Disk after cleanup | `/` 90 % used, 12 GB free; images `0.1.27`, `0.1.25`, `0.1.22` retained |

**Production availability incident:** the first candidate, `0.1.26`, did not serve traffic. It was
replaced in place at ~12:06Z, crash-looped on an IAM database schema mismatch, and was only replaced
by a working `0.1.27` at ~12:45Z, so the production gateway was unavailable for roughly **39
minutes**. No data was lost; PostgreSQL was never recreated. The incident and the fix-forward are
recorded in section 3.

## 1. Workspace dependency wave

Survey: all 105 registered `sdkwork-space` submodules (106 repositories including the unregistered
on-disk checkout) were fetched and advanced to current `main` with the repository tool
`node bin/update-all.mjs --latest --jobs 12 --exclude sdkwork-api-cloud-gateway` (11 m 57 s;
console `sync-0126.log`, JSON `update-all-report-0126.json`). Nothing was reset, rebased,
force-pushed, stashed, checked out to another branch, or cleaned.

| Outcome | Count | Notes |
| --- | --- | --- |
| already current | 98 | no upstream movement |
| advanced | 5 | 294 commits total; `sdkwork-daxuedazi` +284, `sdkwork-birdcoder2` +6, `sdkwork-specs` +2, `sdkwork-documents` +1, `sdkwork-webserver` +1 |
| local commits only (left alone) | 2 | `sdkwork-cloudrouter`, `sdkwork-knowledgebase` |
| blocked / failed | 0 / 0 | |

No advanced repository changed a migration `.sql` file, and no local-ahead or dirty repository did
either, so the only schema movement came from the IAM wave (section 3).

### 1.1 Checkout integrity recovery

The wave tool runs an unconditional `git submodule update --init` on the superproject before the
per-repository advance, and `--exclude` does not cover that step. It left **20 clean submodule
checkouts detached at their remote-main tips** — including the gateway checkout, which it rewound
from branch `main` (`b8fa9ab`) to the superproject-recorded `ea1fbf6`. Every local `main` branch,
and every unpushed local commit, stayed intact (the tool never rewrote a branch).

The release owner inspected all 106 repositories and reattached every detached checkout to its own
branches without discarding anything:

- 18 repositories whose local `main` already contained the detached remote tip were switched back to
  `main` (`git switch main`, fast-forward worktree);
- `sdkwork-knowledgebase` (local `main` five commits ahead of `origin/main`) was switched back to
  `main`;
- `sdkwork-iam` was genuinely diverged (local `main` +1, `origin/main` +3) and was merged forward
  (`fdb3ef17`), preserving both sides — the three upstream commits carry the desktop browser login
  handoff, official-accounts surface and bootstrap-operator provisioning;
- the gateway checkout was restored to `main` (`b8fa9ab`), which also restored the
  `origin/main` commit `b753404` (IAM audience allow-list warning) that the rewind had removed from
  the worktree.

Result: 106/106 repositories on `main`, no merge/rebase in progress, no detached HEAD, dirty
worktrees preserved exactly as found before the wave.

## 2. Release preparation and gates

- Gateway merged `origin/main` `b753404` forward as `b8fa9ab` (the local branch also carries the
  earlier unpushed release commits; nothing was rewritten or squashed).
- Release bumped to `0.1.27` in `agent.app.config.json` (`currentVersion`, `latest.BETA`, release
  notes), `package.json`, `agent.workflow.json` (`defaultVersion` + six artifact paths) and
  `.github/workflows/package.yml`.
- The operator's live IAM login configuration change from earlier today was preserved into the
  repository: the bundle compose templates now pass `SDKWORK_IAM_LOGIN_BASE_URL` into the gateway
  container (from `GATEWAY_IAM_LOGIN_BASE_URL`), and the env examples document the key. Without this,
  the redeploy would have silently dropped the login redirect base the operator had hot-patched onto
  the deployed compose file and configured in `production.env`.
- `bin/*.sh` executable bits were restored so `check-module-bin.mjs` passes (the checkout carried
  them as mode 644 even though `MODULE_BIN_SPEC.md` §2 requires executable scripts).
- Gates before the first build: `cargo metadata --locked --no-default-features --features
  platform-foundation-suite-container` exit 0 in the bookworm container (lock clean), gateway API
  closure 3,433 method/path routes, API assembly integration closure, app manifest + manifest
  deployment + source config + agent workflow + pnpm script + repository docs + module-bin +
  verify-repo standards, and the compose deployment matrix (10 environment/mode combinations).
- Migration pre-flight for production: all 178 applied rows still matched their worktree files, the
  only added migration files were IAM `0024`–`0027`, the OAuth client-id duplicate pre-check returned
  0, and the production `iam_organization` table held a single row in a single tenant.

## 3. Incident and fix-forward — 0.1.26 never served

`0.1.26` was built from the synced wave and deployed in place. It crash-looped with:

```
Error: failed to build embedded router for `sdkwork-iam-app-api`: ensure embedded IAM tenant
application bootstrap failed: ensure default IAM subject failed: error returned from database:
there is no unique or exclusion constraint matching the ON CONFLICT specification
```

Root cause: the IAM wave merged today added migrations `0024`–`0027`; `0027` replaces the global
`iam_organization` primary key `(id)` with `(tenant_id, id)`, which the desktop login feature needs in
order to give every tenant its own `"0"` personal-login organization sentinel. The postgres
default-subject upsert in `sdkwork-iam-bootstrap` still declared `ON CONFLICT(id)`, so PostgreSQL
could no longer resolve the conflict target and the gateway failed closed before binding a port.

The bundle preserved the live `GATEWAY_MIGRATE_ON_START=false`, so the new image served with the old
schema, and the operator's first restart attempt failed immediately. Applying the migrations alone was
not sufficient — after `0024`–`0027` were applied (178 → 182 rows) the same code still failed, because
the SQL itself is incompatible with the new key. The fix is a code fix, not a schema fix.

Repair sequence (all fix-forward; nothing rolled back or discarded):

1. Production migrations applied with a one-time `GATEWAY_MIGRATE_ON_START=true` deploy pass
   (entrypoint `--provision-databases` → `--bootstrap-databases` → `--migrate-databases`), after the
   pre-flight checks in section 2. History went 178 → 182 rows; `iam_organization` now has
   `PRIMARY KEY (tenant_id, id)`, `iam_session.device_id` and `iam_user_official_account` exist. The
   flag was immediately restored to `false` through `bin/config.sh` so the running convention is
   unchanged.
2. `sdkwork-iam` commit `b4cf9142` — the postgres default-organization upsert now uses
   `ON CONFLICT (tenant_id, id)`; the sqlite upsert keeps `ON CONFLICT(id)` because the sqlite schema
   still owns the global primary key (there is no sqlite parity migration for `0027`).
3. `0.1.26` was **not** overwritten: the release advanced to `0.1.27`, the image and bundle were
   rebuilt on the bookworm toolchain, and the broken `0.1.26` image was later removed from the host.
   `0.1.26` never served traffic and is not a release.

Downtime: the previous production container was replaced at ~12:06Z by the failing `0.1.26`; the
IAM merge diagnosis, migration application, IAM fix, and the 22-minute rebuild finished with the
`0.1.27` health gate at ~12:45Z. The gateway was unavailable for approximately 39 minutes. Embedded
PostgreSQL and Redis kept running throughout; no volume was recreated and no data was lost.

## 4. Image and bundle build (`0.1.27`)

Built inside the existing bookworm build container `gw-build-023` (workspace `/work`,
`CARGO_HOME=/opt/cargo-home`, `RUSTUP_HOME=/opt/rustup`,
`CARGO_TARGET_DIR=/work/.build-cache-gateway-target-bookworm`, `CARGO_INCREMENTAL=0`, static
Opus 1.6.1) so the shipped binary matches the runtime glibc/OpenSSL ABI.

- `cargo build --release --locked -p sdkwork-api-cloud-gateway ...` + knowledgebase-rpc build:
  9 m 55 s (warm dependency cache, the IAM crate change recompiled the assembly graph).
- `docker build --no-cache`: completed 12:41:42Z; image `sha256:dddfce7e3ca7…`, 455,633,787 B.
  `ldd` shows only `libssl3`/`libcrypto3`/`libgcc_s`/`libm`/`libc` — no `libopus` runtime dependency.
- Install bundle packaged on the host with `pnpm build:container:install -- --skip-image-build`:
  `image.tar.gz` 453,930,395 B, sha256 `2ea0808e…` matching both `image.sha256` and
  `manifest.imageSha256`; `image.env` and manifest both `0.1.27`.
- Evidence: `build-0127.log`, `package-0127.log`; `bin/docker-image.sh` wrote its lines to
  `target/bin-evidence/evidence.log`.

## 5. Production upgrade

1. Pre-change backup set `sdkwork-api-cloud-gateway-production-20261105T124258Z`
   (`config.tar.gz` 7.8 KB + gateway `-data` volume archive; the PostgreSQL dump is still 0 bytes —
   the pre-existing embedded-mode limitation documented since 0.1.22).
2. Bundle tar-stream push (including the `image.tar.gz`, because the host did not yet carry 0.1.27),
   `docker load`, then compose recreated only the knowledgebase-rpc singleton and the gateway
   instance; embedded postgres/redis were never recreated. The live `bundle/env/production.env` was
   preserved (mtime and values kept: `GATEWAY_ENVIRONMENT=production`, `GATEWAY_HOST_PORT=3913`,
   `GATEWAY_DEPS_MODE=embedded`, `GATEWAY_MIGRATE_ON_START=false`).
3. Health gate: `gateway is healthy (waited 35s)`, exit 0, `evidence.log` line at 12:45:02Z.

## 6. Verification evidence

- `bin/docker-deploy.sh status --environment production`: gateway
  `registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.27`, `Up (healthy)`,
  `0.0.0.0:3913->8080/tcp`; knowledgebase-rpc `:0.1.27` healthy; postgres/redis healthy with
  unchanged images and multi-day uptimes.
- Gateway inspect: `Image=…:0.1.27`, `Health=healthy`, `RestartCount=0`, `OOMKilled=false`,
  `StartedAt=2026-11-05T12:45:00Z`.
- Direct probes on the host (`127.0.0.1:3913`, `Host: gateway`): `/healthz` 200 15 B,
  `/readyz` 200 18 B; the nginx edge (`api.birdcoder.com`) `/healthz` 200.
- `/openapi/index.json`: 200, **1,219,789 B**, **3,424** route entries; the new IAM
  device-authorization routes (`/app/v3/api/oauth/device_authorizations/...`) are present.
- Envelope: unauthenticated `GET /backend/v3/api/iam/tenants` → 401
  `application/problem+json`, `code:40101`, non-empty `traceId`/`instance`; non-allowlisted `Host`
  → 421 `application/problem+json`, `code:40001`, `title:host_not_allowed`.
- Container configuration: `SDKWORK_IAM_LOGIN_BASE_URL=https://platform.birdcoder.com/auth`,
  `SDKWORK_API_CLOUD_GATEWAY_IAM_ALLOWED_AUDIENCES` includes `app_daxuedazi`,
  `SDKWORK_API_CLOUD_GATEWAY_MIGRATE_ON_START=false` — the operator's live IAM configuration reaches
  the new image.
- Startup/runtime log scan of the new container: **0** matches for
  `panic|ERROR|FATAL|failed to compose|failed to build embedded router|checksum_mismatch|ON CONFLICT`.
  The remaining WARN lines are the documented operator-config baseline (RTC credentials unset,
  course entitlement verification unset, OpenAI relay fallback without provider secret resolver,
  payment provider registry skipped, order scheduler already running), not regressions.
- Database: `ops_schema_migration_history` 182 rows, IAM max `0027`, organization PK
  `(tenant_id, id)`, `iam_session.device_id` and `iam_user_official_account` present, no duplicate
  OAuth client ids.
- Storage: image `0.1.26` (never served) and the unused `0.1.23` were removed by explicit
  `docker rmi` of those two tags after the health gate passed; `/` went 93 % → **90 %** (12 GB free).
  `0.1.27` (production), `0.1.25` and `0.1.22` are retained.

## 7. Rollback and recovery

The pre-change backup set and image `0.1.25` are on the host, but **an image-only rollback to 0.1.25 is
not a supported single command after this release**: migration `0027` has been applied, and the
pre-0.1.27 IAM bootstrap SQL also declares `ON CONFLICT(id)`, which the composite primary key no
longer satisfies. Recovery options, in order of preference:

1. Redeploy/keep `0.1.27` (current healthy state) with the retained bundle.
2. Rebuild a pre-0.1.27 image with the IAM `b4cf9142` SQL fix if an older gateway revision is required.
3. Restore the pre-change backup set, or reverse migration `0027`'s primary key with the data-preserving
   procedure documented in the migration (single-tenant today; the sentinel row is preserved).

## 8. Follow-ups (not blocking this release)

1. **Migration/SQL review must gate the image build.** The 0.1.26 failure was catchable before the
   image was pushed: a startup-composition smoke test (or a schema/statement consistency check) against
   the new schema would have failed in seconds. Add a pre-build "boot the candidate image against a
   scratch database with all packaged migrations applied" step.
2. **The wave tool detaches submodule checkouts.** `bin/update-all.mjs` runs an unconditional
   `git submodule update --init` that ignores `--exclude`; it left 20 checkouts detached and rewound
   the gateway. The wave should run the init step before recording a pre-wave snapshot, or restore each
   repository's branch at the end; at minimum the report must fail loudly on a detached HEAD.
3. **Upstream IAM inconsistency.** `sdkwork-iam` main shipped migration `0027` without updating the
   postgres default-subject upsert. The local fix (`b4cf9142`) is not pushed; publish it upstream and
   add a test that runs the IAM bootstrap against a schema with `0027` applied.
4. **Deploy health gate still accepts `running` during a crash loop.** The 0.1.26 apply reported
   success at the gate level (the container was `running` while crash-looping); this release detected
   the loop only through container inspection. Make the gate require Docker `Health=healthy` and
   `RestartCount=0` across a bounded grace window.
5. Embedded-mode pre-change database dump is still empty (`pg_dump` cannot reach the container-only
   PostgreSQL from the host); a real restore point needs the approved `bin/backup.sh` arrangement.
6. The `0.1.25` and `0.1.22` images remain as reference but are not directly bootable against the
   `0027` schema without the IAM fix; document that in the rollback runbook.
7. Doctor `config` FAIL (empty embedded `GATEWAY_CLOUDROUTER_REDIS_PASSWORD`) and the disk WARN remain
   the pre-existing baseline.

## 9. Release evidence paths

| Artifact | Path |
| --- | --- |
| Wave console + JSON | `.deploy-wave-0126/sync-0126.log`, `update-all-report-0126.json` |
| 0.1.26 failure evidence | `.deploy-wave-0127/incident-0126-crashloop.txt` |
| 0.1.27 build | `.deploy-wave-0127/build-0127.log` |
| 0.1.27 bundle package | `.deploy-wave-0127/package-0127.log` |
| Production deploy | `.deploy-wave-0127/deploy-production-0127.log` |
| Independent verification | `.deploy-wave-0127/verify-production-0127.log` |
| Bin evidence | `target/bin-evidence/evidence.log` |

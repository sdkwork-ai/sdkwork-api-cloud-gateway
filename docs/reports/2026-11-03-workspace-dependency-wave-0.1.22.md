# sdkwork-api-cloud-gateway 0.1.22 — workspace dependency wave, production rebuild and upgrade

Date: 2026-11-03 (Asia/Shanghai)
Scope: sync every `sdkwork-space` submodule to current `main`, rebuild the container
delivery on the bookworm toolchain, and upgrade the remote production Docker stack
(`ssh://ubuntu@api.noaper.com`) in place with its live operator env preserved.

## Outcome

| Item | Value |
| --- | --- |
| Release version | `0.1.22` |
| Container image | `registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.22` |
| Image id | `sha256:0f1bf292c732d12ff260e723fffa5a31c959abc1f4c5288021792ba2d0346243` |
| Image size | 433.9 MB (454999123 bytes) |
| Build toolchain | `debian:bookworm-slim@sha256:7c7b2c96…` container, rustc 1.99.1, glibc 2.36 |
| Install bundle | `dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.22.bundle` |
| Bundle image sha256 | `560085e696af2f516324c422b487b887b22ee23890e2f078b6c1193cf0567072` |
| Remote bundle | `/opt/deploy/sdkwork-api-cloud-gateway/bundle` (`image.env` = `0.1.22`) |
| Deploy command | `bash bin/docker-deploy.sh install --environment production --yes --host ssh://ubuntu@api.noaper.com` |
| Deploy result | exit 0 — `gateway is healthy (waited 35s)`, 1 instance applied |
| Remote containers | gateway + knowledgebase-rpc `:0.1.22` healthy; postgres `pgvector/pgvector:pg18` and redis `redis:8-alpine` untouched |
| Health | `/healthz` 200 `{"status":"ok"}`, `/readyz` 200 `{"status":"ready"}` directly and through the public edge |

Three fix-forward repairs were required before a release could serve: a payment
migration-checksum reconciliation, an axum overlapping-route panic in the Cloud
Router/IAM composition, and a missing dependency API surface for the new
`sdkwork-partner` partner-center routes.

## 1. Workspace dependency wave

Survey: 106 discovered sub-projects (105 registered submodules + one on-disk
unregistered checkout). Tool: `node bin/update-all.mjs --latest --jobs 12
--exclude sdkwork-api-cloud-gateway --json-report .sdkwork/update-all-report-1793678237.json`
(8m16s, 143 commits advanced).

Sync result (104 registered submodules excluding this gateway, plus the
unregistered `sdkwork-birdcoder`):

| Action | Count |
| --- | --- |
| advanced to `origin/main` | 41 (40 registered + 1 unregistered) |
| already current | 63 |
| diverged merged forward (`sdkwork-knowledgebase`) | 1 |
| pre-existing dirty worktrees preserved untouched | 3 (`sdkwork-agents`, `sdkwork-app-topology`, `sdkwork-birdcoder2`) |
| blocked / failed | 0 / 0 |

Largest advances: `sdkwork-deployments` +16, `sdkwork-appstore` +13,
`sdkwork-iam` +10, `sdkwork-specs` +10, `sdkwork-agents` +9,
`sdkwork-birdcoder2` +8, `sdkwork-course` +8, `sdkwork-whatseek` +6,
`sdkwork-mall` +5, `sdkwork-order` +5; the remaining 31 advanced 1–5 each.

`sdkwork-knowledgebase` kept its local `Cargo.lock` resync commit and merged
upstream's TLS-trust-bootstrap work forward (`git merge origin/main`, merge
commit `8bf44bc`, no conflicts). The gateway itself was already level with
`origin/main` (10 local release commits ahead, nothing to merge).

Superproject commit `d007774` records the 41 advanced gitlinks (local only,
not pushed); `sdkwork-app-topology`'s dirty tree was never staged.

## 2. Release preparation

- `Cargo.lock` validated locked-clean for the
  `platform-foundation-suite-container` feature graph inside the bookworm
  toolchain (`cargo metadata --locked` passes) — no lock regeneration needed.
- Version bumped to `0.1.22` across `agent.app.config.json` (incl. release
  notes), `package.json`, `agent.workflow.json` (defaultVersion + 6 artifact
  paths) and `.github/workflows/package.yml` defaults.
- Gateway standards pass: app manifest, manifest deployment, source config,
  agent workflow, pnpm script, repository docs, component API surface prefixes,
  API assembly integration closure, gateway API closure (3406 method/path
  routes) and `verify-repo`. The whole-workspace
  `check-api-operation-patterns` gate still reports pre-existing
  `sdkwork-daxuedazi`/other-module violations unrelated to this release.

## 3. Fix-forward repairs (0.1.20 → 0.1.21 → 0.1.22)

The first two build/deploy attempts were discarded and the version advanced
each time (`0.1.20`, `0.1.21` never served traffic; `0.1.19` stayed the
rollback target).

1. **`sdkwork-payment` migration checksum drift (0.1.20).** Production refused
   to bootstrap: `checksum_mismatch for migration 0002` (applied
   `0ad0c442…`, current `bbe634de…`). The wave's payment commit changed only
   rollback metadata comments in `0002_…up.sql`/`0003_…up.sql` (verified with
   a diff: all added lines are `-- rollback:/reversible:/transactional:/…`).
   The live `ops_schema_migration_history` checksums for `payment` `0002` and
   `0003` were reconciled to the current SHA-256 file hashes; the schema was
   already correct, so no DDL was applied. The same reconciliation was applied
   to the development database.

2. **Axum overlapping route panic (0.1.20 → 0.1.21).** Startup panicked at
   `sdkwork-cloudrouter-edge-runtime/src/runtime.rs:1988`:
   `Overlapping method route. Handler for POST /backend/v3/api/iam/api_keys
   already exists`. The new IAM backend assembly owns that IAM route while
   Cloud Router still mounted its legacy command handler. Fixed in
   `sdkwork-cloudrouter` (committed `118324f3`): added
   `PostgresSharedRuntime::include_legacy_iam_api_key_create` (threaded through
   `AdminRouterRuntime`), gating only the legacy `POST` handler while keeping
   the Cloud-Router-owned `DELETE /backend/v3/api/iam/api_keys/{api_key_id}`
   route mounted. The all-in-one platform gateway passes `false`; standalone
   Cloud Router compositions keep the create route.

3. **Missing dependency API surface (0.1.21 → 0.1.22).** Startup failed with
   `failed to compose embedded API assemblies: sdkwork-cloudrouter route
   /app/v3/api/partner_center is not declared by a matching dependency API
   surface`. The new `sdkwork-partner` partner-center app API is composed into
   the platform gateway through Cloud Router's app manifest, so
   `specs/component.spec.json` now declares `/app/v3/api/partner_center` in the
   `sdkwork-cloudrouter-app-api` dependency `apiPrefixes` and in `apiSurfaces`.
   Surface-prefix and closure gates pass with the addition.

## 4. Image and bundle build

Built inside an isolated `debian:bookworm-slim` container (workspace mounted at
`/work`, host rustup toolchain, `CARGO_TARGET_DIR=/work/.build-cache-gateway-target-bookworm`)
so the shipped binary matches the runtime glibc/OpenSSL ABI. The build needs a
static Opus archive (installed into `/usr/lib/x86_64-linux-gnu/libopus.a` with
its `opus.pc`, matching the previous wave's recipe) and autotools for the
fallback source build; `bash bin/docker-image.sh build --image-tag 0.1.22`
completed the cargo release build, knowledgebase-rpc build and `docker build`.
Image binary `ldd` shows only `libssl3`/`libcrypto3`/libc (no `libopus`
runtime dependency). The bundle was packaged with the canonical
`pnpm build:container:install -- --skip-image-build`.

## 5. Production upgrade

Deploy flow: pre-change backup set
`/opt/deploy/sdkwork-api-cloud-gateway/backups/sdkwork-api-cloud-gateway-production-20261103T…Z`
(config + data volume; the PostgreSQL dump is still 0 bytes — pre-existing
backup-credential gap, unchanged), bundle tar-stream push, `docker load` of
`0.1.22`, compose recreate of gateway + knowledgebase-rpc only, health gate.

`deploy.sh` reported `gateway is healthy (waited 35s)`; all four containers are
healthy and the live operator env was preserved (no config rewrite).

Remote disk note: the host was at 90% before the wave and reached 96% after the
failed 0.1.20/0.1.21 images plus 0.1.22 were loaded. Old unused gateway images
were pruned afterwards by explicit tag (keeping `0.1.19` rollback and `0.1.22`
current) to restore headroom.

## 6. Verification evidence

- `bin/docker-deploy.sh status --environment production`: gateway `:0.1.22`
  healthy, `0.0.0.0:3913->8080/tcp`; knowledgebase-rpc `:0.1.22` healthy;
  postgres/redis healthy with unchanged images.
- Direct probes on the host (Host `api.sdkwork.com`): `/healthz` 200,
  `/readyz` 200, `/openapi/index.json` 200, unauthenticated
  `/backend/v3/api/iam/tenants` 401 problem+json, non-whitelisted Host 421.
- Public edge: `https://api.sdkwork.com/healthz` 200,
  `https://api.birdcoder.com/healthz` 200.
- Startup log scan: no `failed to bootstrap`, `panicked`, or overlapping-route
  errors on the current process; the embedded 54-module database bootstrap and
  the composed API-assembly validation both complete.
- `bin/doctor.sh --environment production`: known non-defects only
  (`GATEWAY_CLOUDROUTER_REDIS_PASSWORD` empty for the embedded passwordless
  Redis, logs substring false-positive, disk usage warn).

## 7. Follow-ups (not blocking the release)

1. `sdkwork-payment` (and any similarly edited module) should avoid rewriting
   already-applied migration files; a versioned migration is the correct
   mechanism. The history reconciliation here is a one-off repair.
2. Cloud Router should retire its legacy `/backend/v3/api/iam/*` admin handlers
   entirely (create route already gated); IAM remains the single owner.
3. `sdkwork-partner` should keep its app API surface declarations generated
   alongside the routes so the gateway contract does not lag the code.
4. Pre-change database dump is still empty (`bin/backup.sh` logical dump path).
5. Manifest container checksum in `artifacts.installConfig` remains stale
   (image identity of record is `dist/container-image.json`).

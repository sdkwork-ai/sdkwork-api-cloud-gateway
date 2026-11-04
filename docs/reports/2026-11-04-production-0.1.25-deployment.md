# sdkwork-api-cloud-gateway 0.1.25 — workspace wave, contract fix-forward and production upgrade

Date: 2026-11-04 (Asia/Shanghai)
Scope: sync every `sdkwork-space` submodule to current `main`, rebuild the container delivery on the
bookworm toolchain, and upgrade the remote production Docker stack
(`ssh://ubuntu@api.noaper.com`) in place with its live operator env preserved.

## Outcome

| Item | Value |
| --- | --- |
| Release version | `0.1.25` |
| Container image | `registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.25` |
| Image id | `sha256:9e36df55046394f8200d8b79f467383e3edf61e8c1fa43d586ea96c20e512cca` |
| Image size | 434.3 MB (455,391,563 bytes) |
| Build toolchain | `debian:bookworm-slim` container `gw-build-023`, rustc 1.99.1, glibc 2.36, static Opus 1.6.1 |
| Install bundle | `dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.25.bundle` |
| Bundle image sha256 | `c5c4a2003ea42cfc0dbaecd0f40c170beea2a09286ea107aa6dce3c11f587a42` (453,715,237 B) |
| Remote bundle | `/opt/deploy/sdkwork-api-cloud-gateway/bundle` (`image.env` = `0.1.25`) |
| Deploy command | `bash bin/docker-deploy.sh upgrade --environment production --deps embedded --host ssh://ubuntu@api.noaper.com --yes` |
| Deploy result | exit 0 — `gateway is healthy (waited 35s)`, 1 instance applied, RestartCount 0 |
| Remote containers | gateway + knowledgebase-rpc `:0.1.25` healthy; postgres `pgvector/pgvector:pg18` (3 d) and redis `redis:8-alpine` (5 d) untouched |
| Health | `/healthz` 200 `{"status":"ok"}` (15 B), `/readyz` 200 `{"status":"ready"}` (18 B), direct and through the nginx edge (HTTP + HTTPS) |
| API surface | `/openapi/index.json` 200, 1,217,697 B, 3,417 route patterns (was 1,207,100 B / 3,397) |
| Disk after cleanup | `/` 89 % used, 13 GB free; only `0.1.22` (rollback), `0.1.23` (demo) and `0.1.25` (production) images retained |

The first attempt (`0.1.24`) failed closed at startup; fixing forward required one gateway contract
repair (nine new Cloud Router vendor API prefixes). The 0.1.24 image was discarded and the release
was rebuilt as `0.1.25`. No production data migration ran; the embedded PostgreSQL/Redis volumes were
never replaced.

## 1. Workspace dependency wave

Survey: all 105 registered `sdkwork-space` submodules plus the on-disk unregistered
`sdkwork-birdcoder` checkout (105 repositories after excluding this gateway). Each repository was
fetched and advanced to current `origin/main` with fetch + fast-forward (or merge-forward for a
diverged repository); nothing was reset, rebased, force-pushed, stashed, checked out to another
branch, or cleaned.

| Outcome | Count | Notes |
| --- | --- | --- |
| already current | 95 | no upstream movement |
| fast-forwarded | 8 | 981 commits advanced in total |
| local commits only (left alone) | 2 | `sdkwork-cloudrouter`, `sdkwork-knowledgebase` (unpushed local commits preserved) |
| merged forward / conflicts / blocked | 0 / 0 / 0 | |

Largest advances: `sdkwork-daxuedazi` +698, `sdkwork-birdcoder2` +276, `sdkwork-birdcoder` +2,
`sdkwork-comments` / `sdkwork-documents` / `sdkwork-models` / `sdkwork-news` / `sdkwork-sdk-commons`
+1 each. Pre-existing dirty worktrees (`sdkwork-agents`, `sdkwork-app-topology`,
`sdkwork-birdcoder2`) were preserved untouched; all 105 repositories remain on `main` with no
merge in progress.

`cargo metadata --locked --no-default-features --features platform-foundation-suite-container`
exits 0 inside the bookworm container after the wave, so `Cargo.lock` stays locked-clean (no
regeneration needed). No migration `.sql` file changed in any advanced repository; a full
pre-deploy drift comparison of the live production `ops_schema_migration_history`
(178 postgres rows) against the current dependency migration files matched **178/178**, so the
startup drift gate had no pending reconciliation.

## 2. Incident — the 0.1.24 image failed its composition gate

The first 0.1.24 image (built 14:28–14:46 UTC) booted into a restart loop with:

```
Error: failed to compose embedded API assemblies: sdkwork-cloudrouter route
/alibaba/api/v1/services/aigc/text2image/image-synthesis is not declared by a matching
dependency API surface
```

Root cause: `sdkwork-cloudrouter` merged `origin/main` at **21:54 CST today** (merge commit
`246015ec`), which synced a batch of provider-native vendor routes — Alibaba, ByteDance, Zhipu,
Runway, PixVerse, Luma, Jimeng, Black Forest Labs, Stability AI — into its generated HTTP route
manifest. The 0.1.23 image was built at 03:58 CST, before that merge, so this is the first delivery
that composes those routes. The gateway's `specs/component.spec.json` dependency contract still only
declared the older vendor prefixes, and the runtime's exact-registry composition rejects any
contributed route that is not covered by a declared dependency API surface.

Fix forward (same class as the 0.1.21→0.1.22 `partner_center` repair): declared the nine missing
prefixes in both places the contract requires — the `sdkwork-cloudrouter-open-api` dependency
`apiPrefixes` and nine matching `apiSurfaces` entries:

```
/alibaba/api            /black_forest_labs/v1   /bytedance/api
/jimeng/v1              /luma_ai/dream-machine  /pixverse/openapi
/runway/v1              /stability_ai/v2beta    /zhipu/api
```

Gates after the repair:

- `node scripts/check-gateway-api-closure.mjs` → `gateway API closure passed (3426 method/path routes)` (was exit 1 with 20 uncovered routes).
- `node ../sdkwork-specs/tools/check-api-assembly-integration-closure.mjs --root .` → passed.

Because the 0.1.24 tag was already built (and loaded on the host), it was **not** overwritten: the
version was bumped to 0.1.25 before rebuilding, keeping image identity and rollback targets
addressable. `0.1.24` was never a release: no release note entry exists for it (same convention as
the earlier aborted 0.1.20/0.1.21 attempts).

## 3. Image and bundle build

Both builds ran inside the existing bookworm build container (`gw-build-023`, uid 1000, Docker socket
mounted, workspace at `/work`, `CARGO_TARGET_DIR=/work/.build-cache-gateway-target-bookworm`,
`CARGO_HOME=/opt/cargo-home`, `RUSTUP_HOME=/opt/rustup`, static Opus 1.6.1 at
`/usr/lib/x86_64-linux-gnu/libopus.a` exposed through `opus.pc`) so the shipped binary matches the
runtime glibc/OpenSSL ABI.

- 0.1.24: `cargo build --release --locked -p sdkwork-api-cloud-gateway …` + knowledgebase-rpc build
  + `docker build --no-cache`; 18 m 19 s (14:28:00 → 14:46:47 UTC). Image
  `sha256:6d5d4666ee15…`; discarded after the composition failure.
- 0.1.25: cargo reused the warm dependency cache (only the packaged `specs/component.spec.json`
  changed, not Rust source); 8 m 39 s (15:03:29 → 15:12:09 UTC). Image
  `sha256:9e36df550463…`, `ldd` shows only `libssl3`/`libcrypto3`/`libgcc_s`/`libm`/`libc` (no
  `libopus` runtime dependency).
- Install bundle packaged on the host with
  `pnpm build:container:install -- --skip-image-build`:
  `dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.25.bundle`, `image.env` =
  `SDKWORK_API_CLOUD_GATEWAY_IMAGE_TAG=0.1.25`, `manifest.json` version `0.1.25`, and
  `image.sha256` = `c5c4a200…` matching the re-computed `sha256sum image.tar.gz`.

`bin/docker-image.sh build` appended the build evidence lines for `0.1.24` (14:46:47Z) and `0.1.25`
(15:12:09Z) to `target/bin-evidence/evidence.log`.

## 4. Production upgrade

The upgrade path was `upgrade` (not down/install) with the embedded dependency mode, so the live
operator env and the embedded PostgreSQL/Redis volumes are preserved by design.

1. Pre-change backup set `sdkwork-api-cloud-gateway-production-20261104T151316Z`
   (`config.tar.gz` 6.6 KB + gateway `-data` volume archive). The PostgreSQL dump is still 0 bytes:
   in embedded mode the host `pg_dump` cannot reach the container-only database — the same
   pre-existing backup-credential gap recorded in the 0.1.22 report, unchanged by this wave.
2. Bundle tar-stream push (including the 453,715,237 B `image.tar.gz`, because the host did not yet
   carry `0.1.25`) to `/opt/deploy/sdkwork-api-cloud-gateway/bundle`; the live
   `bundle/env/production.env` was preserved (mtime 2026-11-02, `GATEWAY_ENVIRONMENT=production`,
   `GATEWAY_HOST_PORT=3913`, `GATEWAY_CONTAINER_PORT=8080`, `GATEWAY_DEPS_MODE=embedded`,
   `GATEWAY_MIGRATE_ON_START=false`).
3. `docker load` of `0.1.25`; compose recreated only the knowledgebase-rpc singleton and the
   gateway instance; postgres/redis kept running (uptime 3 d / 5 d).
4. Health gate: `gateway is healthy (waited 35s)`, exit 0, evidence line written at 15:15:58Z.

### 4.1 Incident note — false-positive gate on the aborted 0.1.24 apply

The `0.1.24` apply also recorded `status=0` in `target/bin-evidence/evidence.log` at 15:03:28Z while
the gateway was in fact crash-looping. The bundle health gate accepts a container whose Docker state
is `running` as converged, and a crash-looping container spends most of each cycle in `running`
(with `health: starting`), so a wait can observe that transient state and report success. The
evidence log now carries an appended corrective annotation for that line. Verification for this
release therefore asserts the Docker health status, `RestartCount` and a zero-match startup log scan
— not just the gate's exit code. Follow-up: make the gate require `Health=healthy` (Docker health
state) rather than accepting `running`.

## 5. Verification evidence

Independent post-deploy verification was performed by a separate read-only operator session and by
the release owner:

- `bin/docker-deploy.sh status --environment production`: gateway
  `registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.25`, `Up (healthy)`,
  `0.0.0.0:3913->8080/tcp`; knowledgebase-rpc `:0.1.25` healthy; postgres/redis healthy with
  unchanged images and uptimes.
- Gateway inspect: `Image=…:0.1.25`, `Health=healthy`, `Restart=0`, `OOMKilled=false`,
  `StartedAt=2026-11-04T15:15:22Z`.
- Direct probes on the host (`127.0.0.1:3913`, `Host: gateway` / `Host: api.birdcoder.com`):
  `/healthz` 200 15 B, `/readyz` 200 18 B.
- nginx edge (`api.birdcoder.com`): `/healthz` 200 15 B over HTTP and over HTTPS
  (`--resolve` to 127.0.0.1); upstream unchanged `127.0.0.1:3913`, no nginx reload.
- `/openapi/index.json`: 200, **1,217,697 B**, **3,417** route patterns. The nine new vendor
  surfaces are live in the index (20 routes: `/alibaba` 3, `/bytedance` 3, `/zhipu` 3,
  `/jimeng` 2, `/runway` 2, `/pixverse` 2, `/luma_ai` 2, `/black_forest_labs` 2,
  `/stability_ai` 1) on top of the pre-existing `/anthropic` 11.
- Live route probe: `GET /alibaba/api/v1/tasks/{task_id}` → 401 `application/problem+json` with
  `"code":40101` and `"instance":"GET /alibaba/api/v1/tasks/{task_id}"`, proving the new dependency
  route is composed and enforced rather than 404.
- Envelope: unauthenticated `GET /backend/v3/api/iam/tenants` → 401
  `application/problem+json`, 446 B, `code:40101`, non-empty `traceId`/`x-sdkwork-trace-id`,
  `operationId`. Non-allowlisted `Host` → 421 `application/problem+json`, 215 B, `code:40001`,
  `title:host_not_allowed`, `traceId`.
- Startup/runtime log scan of the gateway container: **0** matches for
  `panic|checksum_mismatch|Overlapping method route|failed to bootstrap|failed to compose|ERROR|FATAL`
  over the first minutes of the new container. Four pre-existing benign WARNs remain (RTC provider
  credentials unset → signaling-only mode; deploy certificate master key unset) — operator config,
  not regressions.
- `bin/doctor.sh --environment production`: 7 PASS (`toolchain`, `bundle`, `compose`, `health`,
  `probe`, `image` matching 0.1.25, `logs`), 1 WARN (`disk` 89 %), 1 FAIL (`config` placeholder
  `GATEWAY_CLOUDROUTER_REDIS_PASSWORD` still `<empty>` for the passwordless embedded Redis). Both
  non-PASS items are the pre-existing baseline documented in the 0.1.22/0.1.23 reports; nothing new
  failed. Exit 70 is therefore the known baseline, not a release regression.
- Demo regression guard: the upgrade touched only `-production-*` compose projects; production was
  independent of demo throughout. During the verification window, at **15:21:14Z**, a separate
  actor in this shared workspace (evidence line `docker-deploy upgrade env=demo`) also upgraded the
  demo environment from `0.1.23` to `0.1.25` using the same bundle. That was not part of this
  release's command sequence; it is recorded here for accuracy because the pre-upgrade verification
  snapshot captured demo on `0.1.23`. Post-change demo checks: gateway and knowledgebase-rpc
  `:0.1.25` `Up (healthy)`, gateway `Restart=0`, `OOMKilled=false`, direct `:3914` `/healthz` 200
  15 B and `/readyz` 200 18 B, `api-demo.birdcoder.com` edge `/healthz` 200, demo
  `/openapi/index.json` 1,217,697 B (identical to production), demo startup log scan free of
  panic/checksum/composition failures (2 pre-existing credential WARNs, 1 slow-statement WARN and
  2 INFO compensation passes only). Embedded demo postgres/redis containers were not recreated.
- Storage: images `0.1.22` (rollback), `0.1.23` (demo) and `0.1.25` (production) retained; the
  broken `0.1.24` and the pre-existing unused `0.1.19` were removed by explicit `docker rmi` of
  those two tags (0 containers each) after the health gate passed. `/` went 93 % → **89 %**
  (13 GB free), `docker system df` reclaimable 2.613 GB (0.1.22 rollback image).

## 6. Follow-ups (not blocking this release)

1. **Cloud Router vendor-route contract lag.** A provider namespace added in `sdkwork-cloudrouter`
   must land in the gateway `specs/component.spec.json` surfaces in the same change. The closure
   check already catches it (`check-gateway-api-closure.mjs`) but runs after the image is built in
   this flow; run it as a pre-build gate in the release pipeline so a 20-minute image build is not
   wasted.
2. **Health gate false positive.** Require the container's Docker health state to be `healthy`
   (or fail after a bounded grace window) instead of accepting `running`; the aborted 0.1.24 apply
   passed the gate while genuinely crash-looping.
3. **Embedded-mode pre-change database dump is still empty** (`pg_dump` cannot reach the
   container-only PostgreSQL from the host). A real restore point for the embedded volumes needs a
   separate approved `bin/backup.sh`/pg_dump arrangement.
4. `sdkwork-cloudrouter` and `sdkwork-knowledgebase` still carry local unpushed commits (their
   upstream pointers were left untouched by the wave); publish them when the workspace is ready.
5. `sdkwork-payment` (and any other module) must stop rewriting already-applied migration files;
   the 0.1.22 history reconciliation remains a one-off repair. This wave changed no migration SQL.
6. Doctor `config` FAIL (`GATEWAY_CLOUDROUTER_REDIS_PASSWORD` `<empty>`) is an intentional
   embedded-mode placeholder; align the env shape if a clean doctor exit code is required.
7. **Shared-workspace concurrency.** This checkout is used by more than one actor: an automatic
   commit of the intermediate `0.1.24` version bump appeared at 22:58 CST, and a separate demo
   upgrade ran at 15:21:14Z while this release's verification was in flight. Neither changed the
   production result, but operators should serialise release windows or use a dedicated checkout so
   evidence attribution stays unambiguous.

## 7. Rollback

The release ledger is absent (`bundle/release-state/`), so use an explicit version rollback; the
`0.1.22` image is still loaded on the host:

```bash
bash bin/docker-deploy.sh rollback --environment production --host ssh://ubuntu@api.noaper.com \
  --to 0.1.22 --yes
```

Rollback restores the image only; the embedded PostgreSQL volume is untouched by both directions, so
no schema migration was applied by this release (all 178 migration checksums matched before the
window and `GATEWAY_MIGRATE_ON_START=false`).

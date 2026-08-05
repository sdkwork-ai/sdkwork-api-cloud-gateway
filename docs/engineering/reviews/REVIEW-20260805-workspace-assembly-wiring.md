# SDKWork API Cloud Gateway — Workspace Assembly Wiring Status

> Date: 2026-08-05
> Scope: executable wiring in `crates/sdkwork-api-cloud-gateway/src/embedded_dependency_routes.rs`
> and the `with_pool` assembly variants added to sibling application workspaces.

## 1. Status

**27 dependency workspaces are now wired** into the gateway's embedded router
(vs 6 at the 2026-07-31 integration audit). Every wired workspace:

- is selected through the component contract (`dependency_surfaces`), not a hard-coded list;
- shares the gateway's **process-wide PostgreSQL pool** (`DatabasePool`) via an
  `assemble_*_with_pool` / `assemble_contribution_with_pool` variant (IM keeps its own
  process-wide singleton pools by design — `OnceLock` sqlx host + r2d2 bundle);
- contributes a complete `ApiAssemblyContribution` (router + route manifest +
  permission catalog + domain context injectors + readiness check);
- has a `foundation-*` Cargo feature and optional dependency in the gateway crate.

## 2. Wired workspaces (27)

| # | Workspace | Assembly entrypoint | Pool mode |
| --- | --- | --- | --- |
| 1 | sdkwork-iam | `assemble_owner_api_surfaces_with_pool` | shared |
| 2 | sdkwork-order | `assemble_api_router_with_pool` | shared |
| 3 | sdkwork-membership | `assemble_api_router_with_pool` | shared |
| 4 | sdkwork-skills | `assemble_api_router_with_pool` | shared |
| 5 | sdkwork-prompts | `assemble_api_router_with_pool` | shared |
| 6 | sdkwork-documents | `assemble_api_router_with_pool` | shared |
| 7 | sdkwork-course | `assemble_api_router_with_pool` | shared |
| 8 | sdkwork-image | `assemble_api_router_with_pool` (+ background processor) | shared |
| 9 | sdkwork-community | `assemble_api_router_with_pool` | shared |
| 10 | sdkwork-dezhou | `assemble_api_router_with_pool` | shared |
| 11 | sdkwork-doudizhu | `assemble_api_router_with_pool` | shared |
| 12 | sdkwork-mahjong | `assemble_api_router_with_pool` (added) | shared |
| 13 | sdkwork-xiangqi | `assemble_api_router_with_pool` | shared |
| 14 | sdkwork-notary | `assemble_api_router_with_pool` (added) | shared |
| 15 | sdkwork-news | `assemble_api_router_with_pool` (added) | shared |
| 16 | sdkwork-drive | `assemble_api_router(pg_pool)` | shared |
| 17 | sdkwork-payment | `assemble_api_router_with_pool` (added) | shared |
| 18 | sdkwork-im | `assemble_api_router()` (process singleton pools) | im-owned |
| 19 | sdkwork-deployments | `assemble_api_router_with_pool` (added) | shared |
| 20 | sdkwork-cloudrouter | `assemble_api_router(ApiAssemblyContext::cloud_gateway())` | edge runtime |
| 21 | sdkwork-inventory | `assemble_api_router_with_pool` (added) | shared |
| 22 | sdkwork-invoice | `assemble_api_router_with_pool` (added) | shared |
| 23 | sdkwork-portal | `assemble_api_router_with_pool` (added) | shared |
| 24 | sdkwork-shop | `assemble_api_router_with_pool` (added) | shared |
| 25 | sdkwork-voice | `assemble_contribution_with_pool` (added) | shared |
| 26 | sdkwork-appstore | `assemble_contribution_with_pool` (added) | shared |
| 27 | sdkwork-webserver | `assemble_api_router(ApiAssemblyContext::cloud_gateway())` | framework |

## 3. Sibling-workspace changes added this round (all non-external)

Each change adds a `with_pool` (or `from_pool`) entrypoint so the gateway can share its
process pool; existing `from_env` paths are preserved.

- `sdkwork-mahjong`: `assemble_api_router_with_pool` + `build_match_store(pool)` refactor; crate dep `sdkwork-database-sqlx`.
- `sdkwork-notary`: `assemble_embedded_notary_application_router_with_pool` (embedded bootstrap) + `assemble_api_router_with_pool` (assembly); `DatabasePool` re-export.
- `sdkwork-news`: `assemble_api_router_with_pool`.
- `sdkwork-payment`: `PaymentServiceHost::from_pool`; `bootstrap_payment_database` made public; `assemble_api_router_with_pool`.
- `sdkwork-deployments`: `bootstrap_deploy_service_host_with_pool`; `assemble_api_router_with_pool`; `repository_from_pool` refactor.
- `sdkwork-inventory` / `sdkwork-invoice`: `bootstrap_*_database_with_pool` (database host), `*ServiceHost::from_pool`, `assemble_api_router_with_pool`.
- `sdkwork-portal` / `sdkwork-shop`: `*ServiceHost::from_pool`, `assemble_api_router_with_pool` (portal in bootstrap.rs, shop in environment.rs).
- `sdkwork-voice`: `assemble_api_router_with_pool` + `assemble_contribution_with_pool` (complete contribution incl. route manifest + readiness).
- `sdkwork-appstore`: `assemble_api_router_with_pool` refactor + `assemble_contribution_with_pool` (uses `appstore_route_manifest()`).
- `sdkwork-im`: `ApiAssemblyRuntime` gains `Drop` (aborts detached `JoinHandle` background workers on gateway shutdown).

## 4. Remaining wiring gaps (documented, not forgotten)

These workspaces are declared in the component contract but are **not yet wired**:

| Workspace | Reason | Required work |
| --- | --- | --- |
| sdkwork-knowledgebase | `KnowledgebaseRuntime::connect` owns RLS + multi-pool bootstrap | add `from_pool` to `KnowledgebaseRuntime` (schema install, drive pool, snowflake lease from caller pool) |
| sdkwork-manager | assembly is router-only and re-assembles IAM + drive itself (conflicts with gateway wiring) | make manager assembly host-neutral (drop internal dependency assembly) |
| sdkwork-appstore-federation / catalog / cms / forum / etc. (15 workspaces) | assembly crate missing complete contribution or gateway mount | materialize `ApiAssemblyContribution` per workspace |

See `docs/product/prd/PRD.md` phase 6 for the remaining rollout.

## 5. Verification

- `cargo check -p sdkwork-api-cloud-gateway --all-features` — passes.
- `cargo test -p sdkwork-api-cloud-gateway --all-features` — 8 tests pass (incl. new
  workspace-surface assertions for the 17 newly wired workspaces).
- The 2026-07-31 audit's "6 wired workspaces" figure is superseded; its P0/P1 findings remain
  resolved as recorded in its header.

## 6. Follow-up additions (same day)

- **Distributed rate limiting**: `sdkwork-utils-rust` gained `rate_limit` (token-bucket
  state machine + Redis Lua atomic buckets, optional `redis` feature). The gateway wires it
  through `traffic.rateLimitBackend = "memory" | "redis"` (+ `redisUrl`/`redisKeyPrefix`).
  Redis connection/response timeouts are bounded (3s/1s) and failures degrade to in-process
  buckets with a `sdkwork_api_cloud_gateway_rate_limit_redis_fallbacks_total` metric.
- **OpenTelemetry export**: `[observability.otel]` (enabled/endpoint/serviceName) exports
  request spans over OTLP/gRPC (opentelemetry 0.31 family, unified with the codex chain).
- **Dependency refresh**: `cargo update` advanced 126 crates (http 1.5, hyper 1.11, serde,
  tokio-macros, ...); rama-error/utils/macros were re-pinned to 0.3.0-alpha.4 (the crates.io
  release lacks `proxy`/`deflate`).
- **Sibling dead-code cleanup**: cloudrouter (unused display-name tables, dead response
  struct, unused chain-policy store), knowledgebase (`mut` fixes + SSRF hostname-layer check
  wired into `resolve_public_socket_addr`), mcp (unused import).
- **Verification**: `pnpm check`, `pnpm verify`, `pnpm workflow:validate`,
  `pnpm gateway:package:standalone`, `pnpm gateway:validate:standalone`, `cargo test
  --workspace`, and `cargo clippy --all-features --tests -D warnings` all pass. On Windows,
  `cargo test --all-features` requires `CARGO_PROFILE_DEV_DEBUG=0` because the combined
  test binary exceeds the MSVC PDB linker limit (LNK1318); CI on Linux is unaffected.

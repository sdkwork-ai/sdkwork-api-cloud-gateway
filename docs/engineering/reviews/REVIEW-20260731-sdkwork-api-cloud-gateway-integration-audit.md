# SDKWork API Cloud Gateway Integration Audit

Status: awaiting human confirmation  
Date: 2026-07-31  
Scope: `E:/sdkwork-space` sibling API applications and `sdkwork-api-cloud-gateway`  
Decision: **not aligned; not release-ready**

## 1. Executive Conclusion

The repository currently describes a broad platform gateway, but the executable runtime does not
implement that declared graph.

| Measure | Result |
| --- | ---: |
| SDKWork repositories inspected by the workspace gateway audit | 93 |
| Workspaces with a canonical `sdkwork-api-*-assembly` crate | 68 |
| Assembly workspaces declared by this gateway | 45 |
| Declared dependency API authorities | 100 |
| Declared canonical dependency prefixes | 215 |
| Workspaces actually wired in `embedded_dependency_routes.rs` | 6 |
| Authorities belonging to those 6 wired workspaces | 15 |
| Prefixes belonging to those 6 wired workspaces | 36 |
| Declared but not wired | 39 workspaces / 85 authorities / 179 prefixes |
| Declared provider workspaces failing gateway-alignment audit | 26 of 45 |
| Declared provider workspaces passing that audit | 18 of 45 |
| Declared provider workspaces with warnings | 1 of 45 (`sdkwork-iam`) |
| API operation-pattern findings in the whole workspace | 1,694 across 37 workspaces |
| API operation-pattern findings in the 45 declared workspaces | 1,274 across 27 workspaces |

The release position is therefore **no-go**. Static manifests are broad, but the executable host,
provider assemblies, Cargo graph, API contracts, runtime-parity evidence, and tests are not closed.

## 2. Normative Baseline

The review used the repository `AGENTS.md`, `sdkwork.app.config.json`, root component contract,
process-pool contract, and these canonical standards:

- `SOUL.md`
- `COMPONENT_SPEC.md`
- `COMPOSABLE_ARCHITECTURE_SPEC.md`
- `APPLICATION_GATEWAY_SPEC.md`
- `APP_RUNTIME_TOPOLOGY_SPEC.md`
- `APP_COMPOSITION_SPEC.md`
- `API_SPEC.md`
- `SDK_SPEC.md`
- `APP_SDK_INTEGRATION_SPEC.md`
- `DEPENDENCY_MANAGEMENT_SPEC.md`
- `RUST_CODE_SPEC.md`
- `TEST_SPEC.md`

The decisive rules are: Cargo is executable linkage authority; dependency APIs remain owner-owned;
the platform gateway embeds approved API assembly contributions; every canonical prefix must be
reachable; metadata alone is not runtime coverage; and completion requires executable parity
evidence and passing checks.

## 3. Blocking Findings

### GATE-P0-001: The executable runtime wires only 6 of 45 declared workspaces

`specs/component.spec.json` declares 45 workspaces, 100 authorities, and 215 dependency prefixes.
`GatewayRuntimeConfig::from_application_root` projects every declared surface into runtime config
(`crates/sdkwork-api-cloud-gateway-config/src/lib.rs:567-586`). The primary builder then composes
the contributions and requires every configured service to be present
(`crates/sdkwork-api-cloud-gateway/src/runtime.rs:200-223`, `845-897`).

The contribution bootstrap has branches only for IAM, Order, Membership, Skills, Prompts, and
Documents (`crates/sdkwork-api-cloud-gateway/src/embedded_dependency_routes.rs:9-66`). It has no
feature-driven registration for the other 39 declared workspaces. Once current build blockers are
removed, exact registry construction will still fail closed with `MissingEmbeddedRouter` before
the listener can serve the complete declared graph.

Impact: 85 declared authorities and 179 declared prefixes have no executable mount path.

### GATE-P0-002: The complete Cargo feature graph does not compile

Both `pnpm check` and direct `platform-foundation-suite` checking fail.

| Workspace | Evidence | Failure |
| --- | --- | --- |
| `sdkwork-image` | `crates/sdkwork-image-generation-runtime-service/src/drive_runtime.rs:47` | passes `Pool<Any>` to an API requiring `Pool<Postgres>` |
| `sdkwork-voice` | `crates/sdkwork-voice-drive-sync-processor/src/lib.rs:124-125` | two `Pool<Any>` to `Pool<Postgres>` mismatches |
| `sdkwork-knowledgebase` | `crates/sdkwork-intelligence-knowledgebase-repository-sqlx/src/postgres_pgvector_retrieval.rs:111` | `push_binding_scope_filters_postgres` is called with 4 instead of 5 arguments |

Impact: the gateway cannot produce an all-features or platform-suite binary.

### GATE-P0-003: 26 declared provider assemblies fail the canonical assembly audit

The workspace gateway-alignment audit reports 26 of the 45 declared workspaces as failing. Most
return router-only/incomplete `ApiAssembly` values missing `route_manifest`, `openapi`,
`permission_catalog`, `domain_context_injectors`, and `readiness_check`. Several standalone hosts
also bypass their assembly with direct route/service/repository/database dependencies.

Failing declared providers:

`sdkwork-account`, `sdkwork-agents`, `sdkwork-aiot`, `sdkwork-appstore`, `sdkwork-browser`,
`sdkwork-catalog`, `sdkwork-cms`, `sdkwork-course`,
`sdkwork-customerservice`, `sdkwork-deployments`, `sdkwork-forum`, `sdkwork-gameengine`,
`sdkwork-image`, `sdkwork-inventory`, `sdkwork-invoice`, `sdkwork-llm`,
`sdkwork-local-router`, `sdkwork-mail`, `sdkwork-manager`, `sdkwork-mcp`, `sdkwork-modelkit`,
`sdkwork-news`, `sdkwork-promotion`, `sdkwork-rtc`, and `sdkwork-voice`.

Impact: these providers are not valid platform assembly contributions even if their Cargo crates
are present and feature-gated.

## 4. High-Severity Findings

### GATE-P1-004: The gateway inventory omits qualified or potential API owners

There are 23 assembly-bearing workspaces not declared by this gateway:

`sdkwork-agentstudio`, `sdkwork-assets`, `sdkwork-audio`, `sdkwork-birdcoder`, `sdkwork-canvas`,
`sdkwork-codebox`, `sdkwork-discovery`, `sdkwork-games`, `sdkwork-github`,
`sdkwork-github-workflow`, `sdkwork-kernel`, `sdkwork-memory`, `sdkwork-merchandise`,
`sdkwork-models`, `sdkwork-notes`, `sdkwork-sdk-generator`, `sdkwork-search`,
`sdkwork-settings`, `sdkwork-terminal`, `sdkwork-tts`, `sdkwork-video`, `sdkwork-video-cut`, and
`sdkwork-web-framework`.

They are not all immediately eligible:

| Classification | Workspaces | Required action |
| --- | --- | --- |
| Structurally ready and API operation checks clean | `sdkwork-birdcoder` | approve ownership/surface scope, then add Cargo feature, contract surfaces, executable contribution, and tests |
| Assembly audit passes but API contract gate fails | `sdkwork-merchandise` | fix 27 operation-id/tag findings before selection |
| Route-bearing assembly fails alignment | `sdkwork-canvas`, `sdkwork-github`, `sdkwork-kernel`, `sdkwork-memory`, `sdkwork-models`, `sdkwork-notes`, `sdkwork-search`, `sdkwork-settings`, `sdkwork-web-framework` | repair owner assembly first |
| App declares API mode none or has no production route contribution | `sdkwork-agentstudio`, `sdkwork-assets`, `sdkwork-audio`, `sdkwork-codebox`, `sdkwork-discovery`, `sdkwork-games`, `sdkwork-github-workflow`, `sdkwork-sdk-generator`, `sdkwork-terminal`, `sdkwork-tts`, `sdkwork-video`, `sdkwork-video-cut` | keep as inventory candidates until owner supplies executable, materialized API evidence |

Four additional OpenAPI-bearing workspaces have no canonical assembly at all:
`sdkwork-comments`, `sdkwork-generations`, `sdkwork-messaging`, and `sdkwork-music`.
They cannot be embedded until their owners publish standard route crates and an API assembly.

### GATE-P1-005: The advertised API closure gate is a false positive

`pnpm gateway:api:validate` passes with 2,601 method/path routes. The implementation in
`scripts/check-gateway-api-closure.mjs:375-503` discovers provider routes from assembly manifests,
SDK manifests, API contracts, or OpenAPI and validates prefix/collision metadata. It never compares
that inventory with `embedded_dependency_routes.rs` or the compiled contribution set.

The corresponding test also passes while 39 declared workspaces have no runtime contribution.
This gate proves contract inventory closure, not executable gateway closure, and its current name
and placement in `pnpm check` overstate readiness.

### GATE-P1-006: The root component contract is not in strict composable form

`check-component-port-bindings --strict` fails because
`specs/component.spec.json:145` has no `contracts.layerRole` (`runtime-gateway` is expected).
It also has no canonical `contracts.requiredPorts` inventory for provider-owned assembly exports.
Instead, `contracts.sdkDependencies` and `contracts.dependencyApiSurfaces` contain indirection
records pointing at custom top-level arrays. This prevents the standard checker from validating
each embedded executable export against a provider port.

Non-strict `verify-repo` passes, which is migration compatibility rather than strict conformance.

### GATE-P1-007: Runtime-parity evidence is absent

`check-api-runtime-parity.mjs --root .` fails because no
`api-runtime-parity.*.evidence.json` exists. There is no machine evidence tying declared OpenAPI,
assembly contributions, compiled features, exact mounted routes, and served OpenAPI together.

### GATE-P1-008: Cargo feature isolation is broken

`cargo test -p sdkwork-api-cloud-gateway --no-default-features` does not compile because
`runtime.rs:219-220` unconditionally references `sdkwork_iam_web_adapter`, while that optional
dependency is activated only by `foundation-appbase`. Composition features therefore are not
independently additive as required by the Rust/composable architecture standards.

### GATE-P1-009: Required tests are stale or test metadata instead of execution

| Test | Result | Cause |
| --- | --- | --- |
| Gateway config tests | 18 pass, 1 fail | membership expects `assemble_api_router_from_env`, contract/provider use `assemble_api_router_with_pool` (`config_tests.rs:527`) |
| Gateway crate tests | 15 pass, 1 fail | source-string assertion still expects `bootstrap_iam_for_application` (`web_framework_tests.rs:37`) |
| Database bootstrap standard | fail | expects IAM to compile exactly one convention database host, but feature resolves zero (`...database-bootstrap-standard.test.mjs:47`) |
| IAM application bootstrap standard | fail | reads removed `src/iam_application_bootstrap.rs` (`...iam-application-bootstrap-standard.test.mjs:14`) |
| Embedded contribution unit test | pass | hard-codes only the same six wired workspaces; it never compares wiring to all declared workspaces (`embedded_dependency_routes.rs:99-116`) |

## 5. Medium-Severity Findings

### GATE-P2-010: Process database-pool contract and runtime inventory drift

`specs/process-database-pool.spec.json` declares 31 consumers; the runtime constant lists 45.
The contract includes four modules absent from the runtime list (`sdkwork-audio`,
`sdkwork-comments`, `sdkwork-messaging`, `sdkwork-music`) and omits 18 runtime-listed modules.
The standard validator still passes, so it does not currently verify contract-to-code equality.

The root component additionally declares a Skills database bootstrap export, but the gateway
member feature does not activate `sdkwork-skills-database-host` and the bootstrap module has no
Skills call. This evidence is declarative only.

### GATE-P2-011: Upstream API operation contracts contain broad standards debt

The workspace operation-pattern validator reports 1,694 findings across 37 workspaces; 1,274 are
inside the gateway's 45 declared workspaces. Finding classes include operationId/tag duplication,
missing/invalid idempotency markers, non-required or malformed `Idempotency-Key`, missing headers,
and action mismatches.

The response-envelope validator passes globally, and the route-collision validator passes globally.
Pagination reports seven findings: one client-side slice in `sdkwork-mall` and six forbidden
`limit` aliases in `sdkwork-web-framework`. Counts include both owner API documents and materialized
SDK OpenAPI copies, so they are file findings, not deduplicated business operations.

## 6. Declared Integration Matrix

`Mounted` means a branch exists in the current executable contribution bootstrap. `Assembly` is
the workspace gateway-alignment result. `API findings` is the operation-pattern file finding count.

| Workspace | Authorities | Mounted | Assembly | API findings |
| --- | ---: | --- | --- | ---: |
| sdkwork-account | 2 | no | fail | 78 |
| sdkwork-agents | 3 | no | fail | 0 |
| sdkwork-aiot | 2 | no | fail | 12 |
| sdkwork-appstore | 3 | no | fail | 0 |
| sdkwork-browser | 2 | no | fail | 10 |
| sdkwork-catalog | 1 | no | fail | 0 |
| sdkwork-cloudrouter | 3 | no | pass | 34 |
| sdkwork-cms | 3 | no | fail | 190 |
| sdkwork-community | 3 | yes | pass | 0 |
| sdkwork-course | 2 | no | fail | 212 |
| sdkwork-customerservice | 2 | no | fail | 0 |
| sdkwork-deployments | 2 | no | fail | 0 |
| sdkwork-dezhou | 2 | no | pass | 4 |
| sdkwork-documents | 3 | yes | pass | 26 |
| sdkwork-doudizhu | 2 | no | pass | 4 |
| sdkwork-drive | 3 | no | pass | 8 |
| sdkwork-forum | 3 | no | fail | 6 |
| sdkwork-gameengine | 2 | no | fail | 4 |
| sdkwork-iam | 3 | yes | warn | 0 |
| sdkwork-im | 3 | no | pass | 12 |
| sdkwork-image | 1 | no | fail | 0 |
| sdkwork-inventory | 2 | no | fail | 12 |
| sdkwork-invoice | 1 | no | fail | 34 |
| sdkwork-knowledgebase | 3 | no | pass | 0 |
| sdkwork-llm | 3 | no | fail | 64 |
| sdkwork-local-router | 3 | no | fail | 0 |
| sdkwork-mahjong | 2 | no | pass | 4 |
| sdkwork-mail | 2 | no | fail | 0 |
| sdkwork-manager | 2 | no | fail | 12 |
| sdkwork-mcp | 2 | no | fail | 8 |
| sdkwork-membership | 2 | yes | pass | 0 |
| sdkwork-modelkit | 1 | no | fail | 0 |
| sdkwork-news | 3 | no | fail | 0 |
| sdkwork-notary | 2 | no | pass | 74 |
| sdkwork-order | 2 | yes | pass | 0 |
| sdkwork-payment | 2 | no | pass | 82 |
| sdkwork-portal | 2 | no | pass | 6 |
| sdkwork-promotion | 2 | no | fail | 26 |
| sdkwork-prompts | 3 | yes | pass | 0 |
| sdkwork-rtc | 2 | no | fail | 44 |
| sdkwork-shop | 2 | no | pass | 303 |
| sdkwork-skills | 2 | yes | pass | 0 |
| sdkwork-voice | 2 | no | fail | 1 |
| sdkwork-web-server | 1 | no | pass | 0 |
| sdkwork-xiangqi | 2 | no | pass | 4 |

Important: `Mounted=yes` is not a pass verdict. Documents has 26 API findings; IAM has an assembly
warning; the complete graph still cannot build or start.

## 7. Verification Record

| Command | Result |
| --- | --- |
| `cargo metadata --locked --no-deps --format-version 1` | pass |
| `cargo fmt -- --check` | pass |
| `pnpm check:standards` | pass |
| `verify-repo.mjs --root .` | pass, non-strict |
| `check-component-port-bindings.mjs --root . --strict` | fail: missing `contracts.layerRole` |
| `check-component-api-surface-prefixes.mjs --root .` | pass |
| `check-route-path-collisions.mjs --workspace ..` | pass |
| `check-api-response-envelope.mjs --workspace ..` | pass |
| `check-api-operation-patterns.mjs --workspace ..` | fail: 1,694 findings |
| `check-pagination.mjs --workspace ..` | fail: 7 findings |
| `check-api-runtime-parity.mjs --root .` | fail: no evidence files |
| `audit-gateway-alignment-workspace.mjs --workspace .. --json` | fail: 36 repositories fail, 1 warns; 26 failures are declared providers |
| `audit-api-assembly-workspace.mjs --workspace .. --strict` | fail: 33 assembly failures |
| `scripts/check-gateway-api-closure.mjs --root .` | pass: 2,601 static routes; false-positive for runtime closure |
| `cargo check ... --features platform-foundation-suite` | fail: Image, Voice, Knowledgebase compile errors |
| `cargo test -p sdkwork-api-cloud-gateway-config` | fail: 18/19 integration tests pass |
| `cargo test -p sdkwork-api-cloud-gateway` | fail: 15/16 tests pass |
| `cargo test -p sdkwork-api-cloud-gateway --no-default-features` | fail to compile: unconditional IAM adapter |
| `pnpm check` | fail at `check:all-features` |
| workflow validation | pass |

`cargo test --workspace`, `pnpm verify`, package, and live `/healthz` smoke were not run to false
completion after their prerequisite build/test gates failed. A live smoke also requires a valid
PostgreSQL profile and cannot reach bind with the current full graph.

## 8. Recommended Remediation Order

1. Confirm the intended platform selection policy: all production API owners, not every directory
   or placeholder assembly. Record explicit approval for business APIs versus infrastructure APIs.
2. Repair provider assemblies first. The 26 declared failing providers must return complete
   `ApiAssemblyContribution` data and pass their own strict assembly/runtime-parity gates.
3. Fix the Image, Voice, and Knowledgebase compile breaks and restore independently additive Cargo
   features, including the no-default-features build.
4. Normalize the gateway component contract to `runtime-gateway`, explicit `requiredPorts`, and
   canonical per-surface dependency records.
5. Replace hand-written six-workspace bootstrap selection with feature-gated assembly contribution
   registration covering every approved declared provider. Add a test that compares Cargo features,
   component dependencies, compiled contributions, exact routes, and served OpenAPI as equal sets.
6. Integrate `sdkwork-birdcoder` after ownership approval. Repair `sdkwork-merchandise` API
   operation contracts before selection. Do not mount the remaining absent candidates until their
   owner contracts and assemblies pass.
7. Repair upstream API operation/idempotency findings and regenerate owner SDKs through the
   canonical generator. Do not hand-edit generated SDK output.
8. Restore all stale tests and make `pnpm check`/`pnpm verify` include strict component ports,
   runtime parity, actual compiled contribution closure, and database contract-to-code parity.
9. Only then run full workspace tests, standalone/cloud package validation, temporary bind smoke,
   `/healthz`, `/readyz`, served OpenAPI parity, and a representative route from every authority.

Cross-repository provider/API fixes, public contract changes, and release-policy decisions require
human review under the repository rules.

## 9. Audit Worktree Note

The worktree already contained uncommitted changes before this audit in `Cargo.lock`, gateway config
tests, an operator guide, knowledgebase RPC dev scripts, the dev runner, and the component contract.
They were treated as owner work and not reverted. During the read-only Cargo verification, Cargo
reported locking two additional packages (`pep440_rs` and `unscanny`) into the already-dirty lockfile;
the pre-audit lockfile diff was not captured, so no partial rollback was attempted. No implementation
or contract fix was made by this review.

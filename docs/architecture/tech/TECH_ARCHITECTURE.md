# SDKWork API Gateway Technical Architecture

Status: active
Owner: SDKWork maintainers
Updated: 2026-07-22
Specs: ARCHITECTURE_DECISION_SPEC.md, DOCUMENTATION_SPEC.md

## Document Map

- [TECH-2026-06-10-commercial-api-gateway-design.md](TECH-2026-06-10-commercial-api-gateway-design.md)
- [TECH-2026-06-10-commercial-api-gateway-phase-1.md](TECH-2026-06-10-commercial-api-gateway-phase-1.md)
- [TECH-2026-06-11-cargo-workspace-gateway-integration-standard.md](TECH-2026-06-11-cargo-workspace-gateway-integration-standard.md)
- [TECH-2026-06-11-shared-gateway-app-migration.md](TECH-2026-06-11-shared-gateway-app-migration.md)
- [TECH-2026-06-12-api-gateway-workspace-layout-alignment.md](TECH-2026-06-12-api-gateway-workspace-layout-alignment.md)

## 1. Architecture Overview

`sdkwork-api-cloud-gateway` is the SDKWork commercial API gateway application. It exposes a single
Axum HTTP surface that embeds dependency assembly routers in-process. The gateway owns health,
readiness, OpenAPI inventory, diagnostics, policy,
observability, and traffic-control behavior; dependency APIs (IAM, drive, commerce, IM, CMS, forum,
and others) remain owned by their application roots.

Two startup modes are first-class:

- **Standalone service** — `crates/sdkwork-api-cloud-gateway` binary
  `sdkwork-api-cloud-gateway`, driven by TOML config and `SDKWORK_API_CLOUD_GATEWAY_*` env overrides.
- **Embedded router** — public builders from `crates/sdkwork-api-cloud-gateway` mount into a host
  Axum application without a separate gateway process.

Development and production are single-port integrated. The `standalone` and `cloud` deployment
profiles package the same embedded assembly graph with different deployment metadata.

## 2. Technology Choices

| Layer | Choice | Notes |
| --- | --- | --- |
| Runtime | Rust + Axum | Workspace members under `crates/` |
| Web pipeline | `sdkwork-web-framework` | Outer request identity, security, CORS, policy, and observability pipeline |
| Config | Typed TOML + env merge | `sdkwork-api-cloud-gateway-config`; prefix `SDKWORK_API_CLOUD_GATEWAY_*` |
| Route registry | In-repo metadata | `sdkwork-api-cloud-gateway-registry` |
| Observability | Prometheus + structured logs | `sdkwork-api-cloud-gateway-observability`; series prefix `sdkwork_api_cloud_gateway_*` |
| Utilities | `sdkwork-utils-rust` | Boolean parsing, path normalization |
| IAM embedding | `foundation-appbase` feature | Optional embedded IAM router via `sdkwork-api-iam-assembly` |
| Packaging | `tar.gz` server archive | `scripts/package-server.mjs`; workflow in `sdkwork.workflow.json` |

Intentionally **not** integrated at this stage:

- `sdkwork-discovery` — deferred until RPC service registration is required.

The host uses `sdkwork-database` process-pool bootstrap for embedded assemblies while owning no
domain tables or migrations itself.

## 3. System Boundaries And Modules

| Crate | Responsibility |
| --- | --- |
| `sdkwork-api-cloud-gateway-config` | Strict embedded runtime config, env binding merge, route path normalization |
| `sdkwork-api-cloud-gateway-registry` | Dependency surface metadata and executable routing rules |
| `sdkwork-api-cloud-gateway-observability` | Startup summaries and observability wiring |
| `sdkwork-api-cloud-gateway` | Router builders, policy chain, embedded routing, web-framework integration |
| `sdkwork-api-cloud-gateway` | Binary entrypoint, config loader, service bootstrap |

Gateway-owned HTTP surfaces (public):

- `/healthz`, `/livez`, `/readyz`
- `/openapi/index.json`, `/openapi.json`
- `/metrics` (when enabled)
- Safe diagnostics inventory (non-secret)

Dependency surfaces are **not** reimplemented in this repository. Appbase IAM routes
(`/app/v3/api/auth`, `/app/v3/api/iam`, `/app/v3/api/oauth`, `/backend/v3/api/iam`, etc.) remain
dependency-owned and are mounted only through their executable assembly exports.

## 4. Directory And Package Layout

Authoritative layout follows `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`:

- `sdkwork.app.config.json` — SDKWork App Standard v3 identity and release declaration
- `specs/component.spec.json` — executable exports, dependency surfaces, Cargo evidence, observability series
- `etc/sdkwork.deployment.config.json` — typed source deployment profile index
- `etc/sdkwork-api-cloud-gateway.{development,test,production}.toml` — safe runtime config
- `apis/gateway-api/` — gateway-owned OpenAPI contract inputs
- `scripts/package-server.mjs` — standalone server packaging and validation

Cargo workspace integration is the gateway catalog authority: root `Cargo.toml`,
`[workspace.dependencies]`, crate features, and public router exports — not a separate catalog file.

## 5. API, SDK, And Data Ownership

- **Gateway-owned API**: health, readiness, OpenAPI inventory, diagnostics, metrics.
- **Dependency APIs**: owned by sibling SDKWork applications; gateway registers embedded routers
  from `specs/component.spec.json` and Cargo workspace evidence.
- **SDK families**: consumers integrate through dependency application SDKs; the gateway does not
  hand-edit generated SDK output.
- **Drive file upload**: delegated to embedded `sdkwork-api-drive-assembly`; no local file storage.

Every dependency surface requires a Cargo feature, gateway assembly dependency, executable router
export, and verified coverage. Legacy split modes and per-dependency base URLs are rejected.

Catalog is embedded through `foundation-catalog` and `sdkwork-api-catalog-assembly`. The
`sdkwork-catalog-app-api` authority is dispatched for `/app/v3/api/addresses`,
`/app/v3/api/cart`, and `/app/v3/api/catalog`; route details remain owned by the Catalog assembly.

## 6. Security, Privacy, And Observability

Security and traffic control run in-process before embedded delegation:

1. Request identity and trace context
2. Route resolution and surface classification
3. CORS, method guard, host guard
4. Header normalization and trusted proxy handling
5. Request/header size policy
6. Security guard and WAF-lite rules
7. Rate limit, quota, concurrency limit
8. Idempotency and replay guard
9. Assembly router selection and readiness enforcement
10. Response headers, rate-limit headers, security headers
11. Metrics, trace spans, access logs, audit/security events

Key env toggles (non-exhaustive): `SDKWORK_API_CLOUD_GATEWAY_SECURITY_HEADERS_ENABLED`,
`SDKWORK_API_CLOUD_GATEWAY_RATE_LIMIT_ENABLED`, `SDKWORK_API_CLOUD_GATEWAY_POLICY_ENABLED`,
`SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED`.

Prometheus series use the `sdkwork_api_cloud_gateway_*` prefix (see `specs/component.spec.json`
observability section). Tracked config templates must not contain live secrets.

## 7. Deployment And Runtime Topology

```mermaid
flowchart LR
  Client --> Gateway["sdkwork-api-cloud-gateway"]
  Gateway -->|embedded| Embedded["Dependency Axum routers"]
```

| Profile | Use case | Packaging |
| --- | --- | --- |
| `cloud` | Multi-service production topology | CI matrix via `sdkwork.workflow.json` |
| `standalone` | Single-node or edge deployment | `pnpm gateway:package:standalone` |

Verification commands (repository root):

- `pnpm verify` — standards, format, workflow, tests, and lint checks
- `pnpm gateway:validate:standalone` — archive manifest and checksum validation
- `cargo test --workspace`

Release builds on Windows may require `CARGO_INCREMENTAL=0` when rustc hits a known `windows-sys`
ICE in incremental release mode.

## 8. Architecture Decision Index

See linked TECH shards in the document map above. Key decisions:

- Cargo-workspace-first gateway integration (TECH-2026-06-11)
- Shared gateway app migration and cloud naming alignment (TECH-2026-06-11, TECH-2026-06-12)
- Commercial policy chain and phase-1 observability/security scope (TECH-2026-06-10)

## 9. Verification

| Check | Command | Expected |
| --- | --- | --- |
| Unit/integration tests | `cargo test --workspace` | All pass |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | Clean |
| Workflow contract | `pnpm workflow:validate` | Pass |
| Full verify | `pnpm verify` | Pass |
| Standalone package | `pnpm gateway:package:standalone` | `dist/server/*.tar.gz` |
| Package manifest | `pnpm gateway:validate:standalone` | Pass |

repository-kind: application

# SDKWork API Gateway

SDKWork API Gateway is a Rust/Axum gateway application. It can run as an independent service or be
embedded into another Rust service through public router builders. The default development topology
is single-port and integrated; foundation dependency APIs are always linked as embedded assemblies.

## Architecture

- `sdkwork-api-cloud-gateway-config`: typed runtime configuration.
- `sdkwork-api-cloud-gateway-registry`: SDKWork route metadata and executable dependency routing rules.
- `sdkwork-api-cloud-gateway-observability`: startup and route summaries.
- `sdkwork-api-cloud-gateway`: Axum router builders, preflight validation, and embedded dependency
  routing.
- `crates/sdkwork-api-cloud-gateway`: independent binary entrypoint and tested service config loader.

Within the gateway crate, endpoint responsibilities are separated into focused modules: `health.rs` owns
`/healthz` and `/readyz`, `openapi.rs` owns OpenAPI metadata responses, and `diagnostics.rs` owns the
safe diagnostics inventory. `runtime.rs` remains the gateway assembly and embedded dispatch layer.

## Root Layout

This repository is a narrow Rust backend application root. Its root layout follows
`../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md` and keeps the full SDKWork project-root directory dictionary
represented with tracked content so active and inactive capabilities are easy to identify:

| Directory | Status | Purpose |
| --- | --- | --- |
| `apis/` | placeholder | Gateway-owned authored API contract inputs when this application owns new API authorities. |
| `apps/` | placeholder | Secondary app surfaces; the repository root is the primary application root. |
| `crates/` | active | Rust gateway, config, registry, observability, and API server crates. |
| `sdks/` | active placeholder | SDK family workspace for gateway-owned API authorities. |
| `jobs/` | placeholder | Schedules, queue bindings, batch descriptors, and maintenance runbooks. |
| `tools/` | placeholder | Reusable validators, generators, migration tools, and operator utilities. |
| `plugins/` | placeholder | Application/runtime plugin source packages. Agent plugins stay in `.sdkwork/plugins/`. |
| `examples/` | placeholder | Runnable and copyable consumer examples. |
| `etc/` | active | Typed deployment profiles, safe TOML runtime config, and non-secret defaults. |
| `deployments/` | placeholder | Deployment descriptors, topology, and release handoff documents. |
| `scripts/` | active | Thin command entrypoints for packaging and workflow integration. |
| `docs/` | active | Design notes, implementation plans, runbooks, and architecture records. |
| `tests/` | placeholder | Repository-level static, contract, integration, and e2e tests. |

Package-local Rust unit and integration tests remain under each crate's `tests/` directory.
`docs/superpowers/` contains historical agent execution records only; current gateway standards live
in this README, `AGENTS.md`, `specs/component.spec.json`, and `../sdkwork-specs/`.

## Foundation API Integration Standard

Foundation API integration is Cargo-workspace-first. The gateway does not use a separate gateway
catalog file: executable integration evidence comes from this repository's `Cargo.toml`
`[workspace.dependencies]`, crate-level Cargo features, and public router exports. SDKWork semantic
evidence stays in `specs/component.spec.json` and dependency SDK assembly metadata.

Every published foundation surface is embedded. The complete machine-readable list is
`specs/component.spec.json`; each entry names its Cargo feature,
assembly dependency, stable API prefix, and executable router export. Representative integrations
include `foundation-drive` with `sdkwork-api-drive-assembly`, `foundation-im` with
`sdkwork-api-im-assembly`, `foundation-cloudrouter` with `sdkwork-api-cloudrouter-assembly`, and
`foundation-skills` with `sdkwork-api-skills-assembly`. Catalog is embedded through
`foundation-catalog` and `sdkwork-api-catalog-assembly`; its app authority owns the gateway
prefix `/app/v3/api/catalog`.

When adding another foundation API, add its assembly crate once in root
`[workspace.dependencies]`, activate it through a focused gateway feature, and record the matching
service, surface, API authority, prefix, and executable export in the component contract. A surface
without executable assembly evidence is not published by this runtime. Do not add per-dependency
base URLs, a proxy fallback, a parallel catalog, or dependency-owned handlers to this product.

## Configuration

`pnpm dev` selects `standalone.development` from `etc/sdkwork.deployment.config.json`. That source
profile selects `etc/sdkwork-api-cloud-gateway.development.toml` and the unified PostgreSQL
development profile from ignored `.env.postgres`. Runtime configuration follows SDKWork precedence:
selected source profile, protected operator config/secrets, process environment, then explicit CLI
overrides. The binary accepts `--config` and `SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE`; the retired
`SDKWORK_API_CLOUD_GATEWAY_CONFIG` key remains a read-only compatibility fallback.

Environment overrides are then applied:

- `SDKWORK_API_CLOUD_GATEWAY_BIND`
- `SDKWORK_API_CLOUD_GATEWAY_MODE`
- `SDKWORK_API_CLOUD_GATEWAY_MAX_BODY_BYTES`
- `SDKWORK_API_CLOUD_GATEWAY_ACCESS_LOG_ENABLED`
- `SDKWORK_API_CLOUD_GATEWAY_AUDIT_LOG_ENABLED`
- `SDKWORK_API_CLOUD_GATEWAY_ACCESS_LOG_SAMPLE_RATE_BASIS_POINTS`
- `SDKWORK_API_CLOUD_GATEWAY_AUDIT_LOG_SAMPLE_RATE_BASIS_POINTS`
- `SDKWORK_API_CLOUD_GATEWAY_METRICS_ENABLED`
- `SDKWORK_API_CLOUD_GATEWAY_TRACING_ENABLED`
- `SDKWORK_API_CLOUD_GATEWAY_SECURITY_HEADERS_ENABLED`
- `SDKWORK_API_CLOUD_GATEWAY_ALLOWED_HOSTS`
- `SDKWORK_API_CLOUD_GATEWAY_RATE_LIMIT_ENABLED`
- `SDKWORK_API_CLOUD_GATEWAY_POLICY_ENABLED`
- `SDKWORK_API_CLOUD_GATEWAY_POLICY_DRY_RUN`
- `SDKWORK_API_CLOUD_GATEWAY_DIAGNOSTICS_ENABLED`
- `SDKWORK_API_CLOUD_GATEWAY_DIAGNOSTICS_PATH`
- `SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED`
- `SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION`
- `SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT`

`sdkwork.app.config.json` declares application and release identity. `specs/component.spec.json`
declares dependency surfaces and Cargo assembly evidence. Source TOML under `etc/` owns bind,
security, traffic, policy, diagnostics, and observability values.

Config loading performs startup validation after TOML/manifest/env merging. The gateway rejects
sample rates above `10000` basis points, zero request or policy body limits, empty dependency
identifiers, incomplete assembly evidence, per-dependency base URL fields, empty traffic/policy rule
ids, and non-positive concurrency limits before serving traffic.

## Run

Preferred SDKWork application entrypoint:

```powershell
pnpm dev
```

`pnpm dev` delegates to `pnpm dev:standalone`, initializes PostgreSQL, and starts the local gateway.
`pnpm dev:cloud` validates the explicit deployed development gateway and starts no local gateway or
database process.

Equivalent Cargo command:

```powershell
cargo run -p sdkwork-api-cloud-gateway --bin sdkwork-api-cloud-gateway -- --config etc/sdkwork-api-cloud-gateway.development.toml
```

## Developer catalog bootstrap

Gateway startup also initializes the platform developer catalog (API market and SDK market) through
the documents database module. The bootstrap resolves the documents assets from
`SDKWORK_DOCUMENTS_APP_ROOT`, `SDKWORK_DATABASE_MODULES_ROOT/sdkwork-documents`, the packaged
`database-modules/` directory, or a sibling `sdkwork-documents` checkout. It runs the module's
init/migrate/seed lifecycle with the same checksum-guarded idempotency as other embedded modules, so
catalog seeds are applied once and skipped on later starts until the seed content changes. Set
`SDKWORK_DEVELOPER_CATALOG_BOOTSTRAP=false` to opt out.

## Server Packaging

The gateway integrates with `sdkwork-github-workflow` through `sdkwork.workflow.json` and the thin
`.github/workflows/package.yml` reusable workflow call. The standard targets are
`linux-x64-standalone-server-tar-gz` and `linux-x64-cloud-server-tar-gz`; workflow dependencies make
the sibling Cargo assemblies available in CI.

Local server archive packaging uses the same lifecycle command:

```powershell
pnpm gateway:package:standalone
pnpm gateway:validate:standalone
pnpm gateway:package:cloud
pnpm gateway:validate:cloud
pnpm workflow:validate
pnpm workflow:matrix:server
```

The gateway package commands package the current host platform by default. CI supplies
`SDKWORK_PACKAGE_PLATFORM`, `SDKWORK_PACKAGE_ARCHITECTURE`, `SDKWORK_PACKAGE_FORMAT`, and
`SDKWORK_PACKAGE_VERSION` through the reusable workflow matrix.

## Test

```powershell
cargo fmt -- --check
cargo check -p sdkwork-api-cloud-gateway --all-features
cargo test --workspace
```

Workspace-scoped rustfmt avoids `--all` traversal of optional foundation path dependencies into
sibling application workspaces that sit outside this repository's Cargo workspace authority.

## SDKWork Surface Rules

The gateway does not reimplement dependency routes. It mounts selected assembly routers through
Cargo features and exposes them on one platform gateway port. Legacy split values and dependency
base URL overrides are rejected during configuration loading.

The runtime generates dependency routes from `dependencySurfaces`: each configured `apiPrefix` is
routed as `apiPrefix/{*path}` to the assembly router registered for that service id. When prefixes
overlap, the registry resolves more specific patterns before broad prefixes. A selected dependency
without a registered embedded router fails preflight instead of falling through to a remote proxy.

Request bodies are bounded by `request.maxBodyBytes` and return `413` with the
`request_body_too_large` problem title when the configured limit is exceeded. Matching
`policy.rules` can apply a stricter `maxBodyBytes` per dependency service or path prefix.

Unmatched or failed gateway routes return RFC 7807-style problem responses with
`application/problem+json`.

## Web Framework Integration

The outer gateway owns request identity, CORS, gateway security, and traffic controls. Embedded
dependency routers are registered by service id and dispatched through fallback delegation; each
assembly owns its dependency authentication and authorization layers and must not duplicate host probes
(`/healthz`, `/livez`, `/readyz`, `/metrics`). Prefer
`sdkwork-<domain>-gateway-assembly::assemble_application_business_router` over raw route-crate
builders when a sibling gateway assembly exists.

Toggle outer web-framework integration with `SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED`
(`true` by default). Router builders `finalize_gateway_router` and
`finalize_gateway_router_async` are exported from `sdkwork_api_cloud_gateway::web_framework`.

Readiness is exposed at `/readyz` and includes the service id, embedded runtime mode, route count,
and dependency surfaces. Assembly registration is verified before the server accepts traffic.

CORS is configured through `[cors]`. `allowAnyOrigin = true` allows any origin; strict mode uses
`allowAnyOrigin = false` with an explicit `allowedOrigins` list.

## Commercial Gateway Controls

Observability is configured through `[observability]`. Access logs are enabled with
`accessLogEnabled` and redact headers listed in `redactHeaders`. Access log events include bounded
failure classes for gateway route misses and embedded dependency errors. `accessLogSampleRateBasisPoints`
controls deterministic access-log sampling; `10000` records all events and `0` records none.
Metrics are disabled by default and can be exposed as Prometheus text with `metricsEnabled = true`
and `metricsPath = "/metrics"`. Problem responses include `traceId` when a valid W3C trace context
is present. The metrics endpoint exports low-cardinality counters for gateway HTTP requests, WAF
decisions, traffic-control rejections, and policy-governance rejections and dry-run observations. It also exports
`sdkwork_api_cloud_gateway_http_request_duration_seconds` as a Prometheus histogram with stable second
buckets from `0.005` through `10` plus `+Inf`; the current measurement is the gateway route duration
until response headers are produced. Metric labels use bounded service, method, status, rule, scope,
action, and event values; they do not include raw paths, request ids, trace ids, tokens, tenant/user
identifiers, or IP addresses.

Diagnostics are configured through `[diagnostics]` and are disabled by default, including in the
production template. When `enabled = true`, the gateway exposes a JSON inventory at `path`, defaulting
to `/internal/gateway/diagnostics`. The response includes only safe configuration metadata: service
name, environment, mode, dependency surface count, route inventory, and policy rule inventory. Route
inventory contains route patterns, service ids, surfaces, execution targets, and methods. Policy
inventory contains `enabled`, `dryRun`, rule ids, match fields, allowed methods, and a boolean flag
that indicates whether a body-size limit exists. It deliberately omits
raw live request paths, request ids, trace ids, headers, tokens, tenant/user/organization
identifiers, IP addresses, and live request data. `includePolicyInventory` and
`includeRouteInventory` can suppress the detailed policy or route item arrays while retaining counts.

Security and traffic decisions also emit structured audit events on tracing target
`sdkwork_api_cloud_gateway::audit` when `auditLogEnabled` is true. `auditLogSampleRateBasisPoints`
controls deterministic audit-log sampling with the same `0` to `10000` range. Audit events use bounded
`category`/`action`/`reason`/`scope` values and include status, method, route target, dependency
service id, request id, and trace context. WAF monitor/block decisions, host allowlist rejections,
header-limit rejections, rate-limit rejections, and concurrency-limit rejections are covered.
Policy method and body-limit rejections are covered with `category = "policy"`.

Security controls are configured through `[security]`. `secureHeadersEnabled` adds baseline response
headers such as `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`,
`Permissions-Policy`, `Content-Security-Policy`, and `Strict-Transport-Security`. `allowedHosts`
enforces a host allowlist and returns `421` with `host_not_allowed` when a request host is rejected.
`maxHeaderBytes` and `maxHeaderCount` reject oversized request headers with `431` and the
`request_headers_too_large` problem title.

`[security.waf]` enables a baseline in-process WAF. In `mode = "monitor"`, suspicious query strings
or headers are allowed through and responses include `X-SDKWork-WAF-Action: monitor`. In
`mode = "enforce"` or `mode = "block"`, matching requests return `403` with the
`waf_request_blocked` problem title and `X-SDKWork-WAF-Action: block`. The Phase 2 baseline detects
common script, SQL injection, path traversal, and sensitive file probes in query strings and headers;
body inspection remains a later hardening step.

Traffic controls are configured through `[traffic]`. `rateLimitEnabled` activates in-memory
token-bucket rules from `traffic.rateLimits`; rejected requests return `429` by default with
`Retry-After`, `RateLimit`, and `RateLimit-Policy` headers. `globalMaxInFlight` and
`traffic.concurrencyLimits` enforce in-memory concurrency limits; rejected requests return `503`
with the `concurrency_limit_exceeded` problem title. These stores are process-local and intended as
Phase 1 defaults; shared Redis-backed policy state belongs in a later deployment phase.

Policy governance is configured through `[policy]`. Empty `rules = []` is a no-op. Each rule can
match `matchServiceId` and/or `matchPathPrefix`, then enforce `allowedMethods` or a stricter
`maxBodyBytes`. Disallowed methods return `405` with the
`method_not_allowed` problem title and an `Allow` header. Policy body-limit rejections return `413`
with `request_body_too_large`. Policy rejections are counted in
`sdkwork_api_cloud_gateway_policy_rejections_total{service,kind,scope,status}` where `scope` is the bounded
policy rule id, never the raw request path.

`dryRun = true` enables policy observation mode for rollout. In dry-run, matched method violations
are forwarded to the dependency instead of being rejected, and stricter policy `maxBodyBytes`
overrides are observed without replacing the global `request.maxBodyBytes` safety limit. When
`Content-Length` proves the request would exceed a policy body limit, the gateway records a
`request_body_too_large` dry-run observation while still forwarding the request if it is within the
global limit. Dry-run responses receive `X-SDKWork-Policy-Action: dry-run`,
`X-SDKWork-Policy-Kind`, and `X-SDKWork-Policy-Rule`, an audit event is emitted with
`action = "dry-run"`, and
`sdkwork_api_cloud_gateway_policy_dry_run_total{service,kind,scope,status}` is incremented with the actual
response status. Dry-run labels are bounded to the policy kind and rule id.

Example:

```toml
[policy]
enabled = true
dryRun = false

[[policy.rules]]
id = "appbase-auth-write-policy"
matchServiceId = "sdkwork-iam-app-api"
matchPathPrefix = "/app/v3/api/auth"
allowedMethods = ["POST"]
maxBodyBytes = 4096
```

## Public Rust APIs

Standalone service startup uses:

```rust
use sdkwork_api_cloud_gateway::build_sdkwork_api_cloud_gateway_router;
```

Third-party embedding can use:

```rust
use sdkwork_api_cloud_gateway::{
    build_sdkwork_api_cloud_gateway_router_with_embedded_routers,
    build_embedded_sdkwork_iam_app_api_router,
};
```

The API server crate exposes `load_gateway_config` for callers that want the same manifest/config/env
resolution used by the binary.

## Documentation Canon

- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)

## Application Roots

- [apps directory index](apps/README.md)

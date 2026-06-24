# SDKWork API Gateway

SDKWork API Gateway is a Rust/Axum gateway application. It can run as an independent service or be
embedded into another Rust service through public router builders. The default development topology
is single-port and integrated; split multi-upstream validation is internal-only.

## Architecture

- `sdkwork-api-cloud-gateway-config`: typed runtime configuration.
- `sdkwork-api-cloud-gateway-registry`: SDKWork route metadata and executable dependency routing rules.
- `sdkwork-api-cloud-gateway-observability`: startup and route summaries.
- `sdkwork-api-cloud-gateway`: Axum router builders, preflight validation, embedded routing, and
  split-mode HTTP/WebSocket proxying.
- `crates/sdkwork-api-cloud-gateway-api-server`: independent binary entrypoint and tested service config loader.

Within the gateway crate, endpoint responsibilities are split into focused modules: `health.rs` owns
`/healthz` and `/readyz`, `openapi.rs` owns OpenAPI metadata responses, and `diagnostics.rs` owns the
safe diagnostics inventory. `runtime.rs` remains the gateway assembly and proxy execution layer.

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
| `configs/` | active | Safe TOML config templates and non-secret runtime defaults. |
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
evidence stays in the existing `sdkwork.app.config.json`, `specs/component.spec.json`, and dependency
SDK assembly metadata.

Current foundation integrations:

| Cargo feature | Cargo dependency | SDKWork service | Router export |
| --- | --- | --- | --- |
| `foundation-appbase` | `sdkwork_router_iam_app_api` | `sdkwork-appbase-app-api` | `sdkwork_router_iam_app_api::build_sdkwork_appbase_app_api_router` |

Representative split upstream foundation surfaces. The complete machine-readable list is
`sdkwork.app.config.json` plus `specs/component.spec.json`:

| SDKWork service | SDK family | API prefix | Base URL key |
| --- | --- | --- | --- |
| `sdkwork-drive-app-api` | `sdkwork-drive-app-sdk` | `/app/v3/api/drive` | `SDKWORK_DRIVE_APP_API_BASE_URL` |
| `sdkwork-notary-app-api` | `sdkwork-notary-app-sdk` | `/app/v3/api/notary` | `SDKWORK_NOTARY_APP_API_BASE_URL` |
| `sdkwork-appbase-backend-api` | `sdkwork-appbase-backend-sdk` | `/backend/v3/api/iam` | `SDKWORK_APPBASE_BACKEND_API_BASE_URL` |
| `sdkwork-commerce-app-api` | `sdkwork-commerce-app-sdk` | `/app/v3/api` | `SDKWORK_COMMERCE_APP_API_BASE_URL` |
| `sdkwork-commerce-backend-api` | `sdkwork-commerce-backend-sdk` | `/backend/v3/api` | `SDKWORK_COMMERCE_BACKEND_API_BASE_URL` |
| `sdkwork-rtc-app-api` | `sdkwork-rtc-app-sdk` | `/app/v3/api/rtc` | `SDKWORK_RTC_APP_API_BASE_URL` |
| `sdkwork-rtc-backend-api` | `sdkwork-rtc-backend-sdk` | `/backend/v3/api/rtc` | `SDKWORK_RTC_BACKEND_API_BASE_URL` |
| `sdkwork-agent-app-api` | `sdkwork-agent-app-sdk` | `/app/v3/api/ai` | `SDKWORK_AGENT_APP_API_BASE_URL` |
| `sdkwork-agent-backend-api` | `sdkwork-agent-backend-sdk` | `/backend/v3/api/ai` | `SDKWORK_AGENT_BACKEND_API_BASE_URL` |
| `sdkwork-agent-open-api` | `sdkwork-agent-sdk` | `/agent/v3/api` | `SDKWORK_AGENT_OPEN_API_BASE_URL` |
| `sdkwork-aiot-app-api` | `sdkwork-aiot-app-sdk` | `/app/v3/api/iot` | `SDKWORK_AIOT_APP_API_BASE_URL` |
| `sdkwork-aiot-backend-api` | `sdkwork-aiot-backend-sdk` | `/backend/v3/api/iot` | `SDKWORK_AIOT_BACKEND_API_BASE_URL` |
| `sdkwork-drive-backend-api` | `sdkwork-drive-backend-sdk` | `/backend/v3/api/drive` | `SDKWORK_DRIVE_BACKEND_API_BASE_URL` |
| `sdkwork-drive-open-api` | `sdkwork-drive-sdk` | `/open/v3/api/drive` | `SDKWORK_DRIVE_OPEN_API_BASE_URL` |
| `sdkwork-drive-admin-storage-api` | `sdkwork-drive-admin-storage-sdk` | `/admin/v3/api/drive` | `SDKWORK_DRIVE_ADMIN_STORAGE_API_BASE_URL` |
| `sdkwork-memory-app-api` | `sdkwork-memory-app-sdk` | `/app/v3/api/memory` | `SDKWORK_MEMORY_APP_API_BASE_URL` |
| `sdkwork-memory-backend-api` | `sdkwork-memory-backend-sdk` | `/backend/v3/api/memory` | `SDKWORK_MEMORY_BACKEND_API_BASE_URL` |
| `sdkwork-memory-open-api` | `sdkwork-memory-sdk` | `/mem/v3/api/memory` | `SDKWORK_MEMORY_OPEN_API_BASE_URL` |
| `sdkwork-knowledgebase-app-api` | `sdkwork-knowledgebase-app-sdk` | `/app/v3/api/knowledge` | `SDKWORK_KNOWLEDGEBASE_APP_API_BASE_URL` |
| `sdkwork-knowledgebase-backend-api` | `sdkwork-knowledgebase-backend-sdk` | `/backend/v3/api/knowledge` | `SDKWORK_KNOWLEDGEBASE_BACKEND_API_BASE_URL` |
| `sdkwork-knowledgebase-open-api` | `sdkwork-knowledgebase-sdk` | `/knowledge/v3/api` | `SDKWORK_KNOWLEDGEBASE_OPEN_API_BASE_URL` |
| `sdkwork-news-open-api` | `sdkwork-news-sdk` | `/open/v3/api/news` | `SDKWORK_NEWS_OPEN_API_BASE_URL` |
| `sdkwork-notes-open-api` | `sdkwork-notes-sdk` | `/notes/v3/api` | `SDKWORK_NOTES_OPEN_API_BASE_URL` |
| `sdkwork-community-open-api` | `sdkwork-community-sdk` | `/community/v3/api` | `SDKWORK_COMMUNITY_OPEN_API_BASE_URL` |
| `sdkwork-image-open-api` | `sdkwork-image-sdk` | `/image/v3/api` | `SDKWORK_IMAGE_OPEN_API_BASE_URL` |

When adding another foundation API, add its source crate once in root `[workspace.dependencies]`,
activate it through a focused gateway runtime feature only when the dependency exposes a compatible
public router builder, and record the matching SDKWork service, surface, API authority, prefix, and
router export in the existing manifest/component spec. Split upstream surfaces that are proxied by
the gateway use the existing dependency surface and upstream config instead of fake Cargo feature
evidence. Do not create a parallel gateway catalog or reimplement dependency-owned handlers in this
product.

Commerce app/backend surfaces are broad fallback routes while the workspace migrates product apps to
the shared gateway. Fixed Appbase IAM routes and more specific app dependency prefixes such as Drive
Notary, RTC, Agent, AIoT, Drive backend/open/admin, Memory, Knowledgebase, News, Notes, Community,
Image, and other specific foundation prefixes are resolved before the broad Commerce fallbacks.
Cargo metadata currently proves the embedded Appbase app surface only; Appbase backend, Drive,
Notary, Commerce, RTC, Agent, AIoT, Memory, Knowledgebase, and the remaining foundation modules
remain split upstream integrations until their production routers expose compatible, approved public
builders.

## Configuration

Configuration is resolved in this order:

1. `--config <path>` or `--config=<path>`
2. `SDKWORK_API_CLOUD_GATEWAY_CONFIG`
3. `sdkwork.app.config.json` in the application root
4. built-in defaults

Environment overrides are then applied:

- `SDKWORK_API_CLOUD_GATEWAY_BIND`
- `SDKWORK_API_CLOUD_GATEWAY_MODE`
- `SDKWORK_API_CLOUD_GATEWAY_MAX_BODY_BYTES`
- `SDKWORK_API_CLOUD_GATEWAY_TIMEOUT_MS`
- `SDKWORK_API_CLOUD_GATEWAY_READINESS_CHECK_UPSTREAMS`
- `SDKWORK_API_CLOUD_GATEWAY_READINESS_TIMEOUT_MS`
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
- `SDKWORK_API_CLOUD_GATEWAY_RETRY_ENABLED`
- `SDKWORK_API_CLOUD_GATEWAY_RETRY_MAX_ATTEMPTS`
- `SDKWORK_API_CLOUD_GATEWAY_CIRCUIT_BREAKER_ENABLED`
- `SDKWORK_API_CLOUD_GATEWAY_CIRCUIT_BREAKER_FAILURE_THRESHOLD`
- `SDKWORK_API_CLOUD_GATEWAY_CIRCUIT_BREAKER_OPEN_SECONDS`
- `SDKWORK_API_CLOUD_GATEWAY_CIRCUIT_BREAKER_HALF_OPEN_MAX_ATTEMPTS`
- `SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED`
- dependency base URL keys such as `SDKWORK_APPBASE_APP_API_BASE_URL`,
  `SDKWORK_DRIVE_APP_API_BASE_URL`, `SDKWORK_NOTARY_APP_API_BASE_URL`,
  `SDKWORK_APPBASE_BACKEND_API_BASE_URL`, `SDKWORK_COMMERCE_APP_API_BASE_URL`, and
  `SDKWORK_COMMERCE_BACKEND_API_BASE_URL`, plus shared foundation keys
  `SDKWORK_RTC_APP_API_BASE_URL`, `SDKWORK_RTC_BACKEND_API_BASE_URL`,
  `SDKWORK_AGENT_APP_API_BASE_URL`, `SDKWORK_AGENT_BACKEND_API_BASE_URL`,
  `SDKWORK_AGENT_OPEN_API_BASE_URL`, `SDKWORK_AIOT_APP_API_BASE_URL`,
  `SDKWORK_AIOT_BACKEND_API_BASE_URL`, `SDKWORK_DRIVE_OPEN_API_BASE_URL`,
  `SDKWORK_MEMORY_OPEN_API_BASE_URL`, `SDKWORK_KNOWLEDGEBASE_OPEN_API_BASE_URL`,
  and other dependency surface keys declared in `sdkwork.app.config.json`

`sdkwork.app.config.json` drives the default SDKWork identity, bind address, API surfaces, and
dependency SDK surfaces. TOML files under `configs/` are tracked templates for environment-specific
deployment overlays.

Config loading performs startup validation after TOML/manifest/env merging. The gateway rejects
sample rates above `10000` basis points, zero request or policy body limits, empty upstream or
dependency identifiers, empty traffic/policy rule ids, and non-positive concurrency limits before
serving traffic.

## Run

Preferred SDKWork application entrypoint:

```powershell
pnpm dev
```

Equivalent Cargo command:

```powershell
cargo run -p sdkwork-api-cloud-gateway-api-server --bin sdkwork-api-cloud-gateway -- --config configs/sdkwork-api-cloud-gateway.development.toml.example
```

## Server Packaging

The gateway integrates with `sdkwork-github-workflow` through `sdkwork.workflow.json` and the thin
`.github/workflows/package.yml` reusable workflow call. The standard server target is
`linux-x64-server-tar-gz`, and the workflow declares `sdkwork-appbase` as a release dependency so the
Cargo sibling path used by the gateway workspace is available in CI.

Local server archive packaging uses the same lifecycle command:

```powershell
pnpm package:server
pnpm package:server:validate
pnpm workflow:validate
pnpm workflow:matrix:server
```

`pnpm package:server` packages the current host platform by default. CI supplies
`SDKWORK_PACKAGE_PLATFORM`, `SDKWORK_PACKAGE_ARCHITECTURE`, `SDKWORK_PACKAGE_FORMAT`, and
`SDKWORK_PACKAGE_VERSION` through the reusable workflow matrix.

## Test

```powershell
cargo fmt -- --check
cargo test --workspace
```

Workspace-scoped rustfmt avoids `--all` traversal of optional foundation path dependencies into
sibling application workspaces that sit outside this repository's Cargo workspace authority.

## SDKWork Surface Rules

The gateway does not reimplement appbase IAM routes. The default development mode is embedded and
auto-mounts the executable appbase IAM router through `foundation-appbase`, so normal local work
uses one public gateway port. Split upstream proxying remains available for lower-level validation
paths, but it is not the standard development workflow.

For non-appbase dependency surfaces, the runtime generates dependency routes from
`dependencySurfaces`: each configured `apiPrefix` is routed as `apiPrefix/{*path}`. When dependency
prefixes overlap, the registry resolves more specific patterns before broad fallback prefixes.
Embedded routers take precedence over split upstream HTTP proxying for the same dependency service
id. In split mode, WebSocket upgrade requests on configured dependency prefixes are proxied to the
dependency upstream with bidirectional message bridging. Close frames preserve the close code and
reason.

HTTP proxy requests add `X-Forwarded-Host` and `X-Forwarded-Proto` before forwarding upstream.
Split mode runs the `sdkwork-web-framework` request-identity stage and assigns a server-generated
UUID v4 `X-Request-Id` on every request. The gateway forwards that value to dependency upstreams and
returns it on proxied HTTP responses and problem details. Client-supplied request ids are not
preserved in split mode because framework identity assignment is authoritative.

Request bodies are bounded by `request.maxBodyBytes` and return `413` with the
`request_body_too_large` problem title when the configured limit is exceeded. Matching
`policy.rules` can apply a stricter `maxBodyBytes` per dependency service or path prefix. Proxied
upstream response bodies are streamed back to the caller instead of being fully buffered by the
gateway. Gateway-level request timeout is configured with `request.timeoutMs`; `0` disables it.
Matching `policy.rules` can override the gateway budget with `timeoutMs`. When enabled, split-mode
dependency requests that exceed this gateway budget return `504` with the `gateway_request_timeout`
problem title.

Unmatched or failed gateway routes return RFC 7807-style problem responses with
`application/problem+json`.

## Web Framework Integration

Split mode mounts the standard `sdkwork-web-framework` interceptor pipeline before gateway proxy
dispatch. Dependency API prefixes are treated as public at the gateway boundary so credential
resolution and authentication remain upstream responsibilities. Unclassified paths are also treated
as public so hidden diagnostics and route-not-found handlers can return gateway-owned problem
responses instead of framework `401` rejections.

Embedded mode applies request identity only on the outer gateway router. Embedded dependency routers
already mount their own framework layers and must not be double-wrapped during gateway merge.

Toggle the outer framework pipeline with `SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED` (`true` by
default). When disabled, split mode falls back to the legacy Tower `CorsLayer` and gateway security
middleware for CORS and secure response headers while still assigning UUID `x-request-id` on the outer
router. Router builders `finalize_gateway_router`, `finalize_gateway_router_async`, and
`split_proxy_call_chain` are exported from `sdkwork_api_cloud_gateway::web_framework`.

Each upstream can set `timeoutMs`/`timeout_ms` through the typed runtime config. Timed out upstream
requests return `504` with the `upstream_timeout` problem title. `request.timeoutMs` is the
gateway-level budget around the proxied dependency request and can fire before a longer per-upstream
timeout.

Readiness is exposed at `/readyz` and includes the service id, runtime mode, route count, configured
upstreams, and dependency surfaces. When `readiness.checkUpstreams` is enabled, required upstreams
are checked with `upstreams[].readinessPath` and failures return `503` with `status: "degraded"`.

CORS is configured through `[cors]`. `allowAnyOrigin = true` allows any origin; strict mode uses
`allowAnyOrigin = false` with an explicit `allowedOrigins` list. In split mode with the web framework
enabled, CORS enforcement and response headers are owned by `sdkwork-web-framework`
`SecurityPolicy.cors` rather than a duplicate Tower `CorsLayer`.

## Commercial Gateway Controls

Observability is configured through `[observability]`. Access logs are enabled with
`accessLogEnabled` and redact headers listed in `redactHeaders`. Access log events include a bounded
`failureClass` when the gateway can classify a failure, including gateway route misses, dependency
timeouts, retry exhaustion, and circuit-breaker short circuits. `accessLogSampleRateBasisPoints`
controls deterministic access-log sampling; `10000` records all events and `0` records none.
Metrics are disabled by default and can be exposed as Prometheus text with `metricsEnabled = true`
and `metricsPath = "/metrics"`. Inbound `traceparent` headers are forwarded upstream and problem
responses include `traceId` when a valid W3C trace context is present. The metrics endpoint exports
low-cardinality counters for
gateway HTTP requests, upstream dependency requests, WAF decisions, traffic-control rejections, and
policy-governance rejections and dry-run observations, plus resilience events such as gateway request
timeout, upstream timeout, retry attempt, retry exhausted, circuit-open, short-circuit, and half-open
probe decisions. It also exports
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
inventory contains `enabled`, `dryRun`, rule ids, match fields, allowed methods, and boolean flags
that indicate whether body-size or timeout limits exist. It deliberately omits upstream base URLs,
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
`Permissions-Policy`, `Content-Security-Policy`, and `Strict-Transport-Security`. In split mode with
the web framework enabled, baseline headers are applied by the framework `HeaderSecurity` stage and
gateway-owned extensions such as `Content-Security-Policy` and
`X-Permitted-Cross-Domain-Policies` remain gateway responsibilities. `allowedHosts`
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
match `matchServiceId` and/or `matchPathPrefix`, then enforce `allowedMethods`, a stricter
`maxBodyBytes`, or a route-specific gateway `timeoutMs`. Disallowed methods return `405` with the
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
matchServiceId = "sdkwork-appbase-app-api"
matchPathPrefix = "/app/v3/api/auth"
allowedMethods = ["POST"]
maxBodyBytes = 4096
timeoutMs = 250
```

Resilience controls are configured through `[resilience]`. `retryEnabled` activates split-mode
upstream retry for statuses listed in `retryStatuses`; `maxAttempts` caps total attempts and
`retryBackoffMs` adds a simple per-attempt backoff. When a later attempt succeeds, proxied responses
include `X-SDKWork-Gateway-Retry-Attempts`. Retry attempts and exhausted retry budgets are counted
as bounded resilience events. Gateway request timeouts and per-upstream timeouts are also counted as
separate resilience events so operations can tell which budget expired. `circuitBreakerEnabled`
activates per-dependency in-memory circuit breaking for split-mode upstreams. Consecutive upstream
`5xx` responses are counted against `failureThreshold`; once open, requests are short-circuited with
`503`, the `circuit_breaker_open` problem title, `Retry-After`, and
`X-SDKWork-Circuit-State: open`. After `openSeconds`, up to `halfOpenMaxAttempts` probe requests are
allowed; half-open probe responses include `X-SDKWork-Circuit-State: half-open`, and a successful
probe closes the circuit.

## Public Rust APIs

Standalone service startup uses:

```rust
use sdkwork_api_cloud_gateway::build_sdkwork_api_cloud_gateway_api_server_router;
```

Third-party embedding can use:

```rust
use sdkwork_api_cloud_gateway::{
    build_sdkwork_api_cloud_gateway_router_with_embedded_routers,
    build_embedded_sdkwork_appbase_app_api_router,
};
```

The API server crate exposes `load_gateway_config` for callers that want the same manifest/config/env
resolution used by the binary.

## Documentation Canon

- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)


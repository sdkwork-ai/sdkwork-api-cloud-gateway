> Migrated from `docs/superpowers/specs/2026-06-10-commercial-api-gateway-design.md` on 2026-06-24.
> Owner: SDKWork maintainers

# Commercial API Gateway Design

## Goal

Bring `sdkwork-api-cloud-gateway` from a working SDKWork dependency gateway to a commercial-grade gateway
foundation with observable, secure, traffic-controlled, resilient, and performant runtime behavior
while preserving both startup modes:

- standalone service through `crates/sdkwork-api-cloud-gateway`
- same-process embedding through `sdkwork-api-cloud-gateway` router builders

## Constraints

- Do not reimplement appbase IAM handlers.
- Appbase IAM remains a dependency surface.
- Split mode requires upstream dependency base URLs.
- Embedded mode requires executable dependency routers.
- Public Rust router builders remain stable.
- Tracked config templates must not contain live secrets.
- Local/test mode must not require Redis or an external gateway.

## Recommended Architecture

Use an in-process Rust policy chain in the Axum runtime, with store and exporter traits for
production integrations.

The policy chain runs before dependency proxying or embedded router delegation:

1. request identity and trace context
2. route resolution and surface classification
3. CORS, method guard, host guard
4. header normalization and trusted proxy handling
5. request size and header size policy
6. security guard and WAF-lite monitor/block rules
7. rate limit, quota, and concurrency limit
8. idempotency and replay guard
9. upstream selection, timeout, retry, circuit breaker, health
10. response headers, rate-limit headers, and security headers
11. metrics, trace spans, access logs, and audit/security events

## Modules

- `config`: typed config for policy, observability, security, traffic, and resilience.
- `runtime::policy`: request policy context, stage execution, and response decisions.
- `runtime::security`: host/trusted proxy/header/method/credential guards and WAF-lite.
- `runtime::traffic`: rate limiting, concurrency limiting, quota hooks, and rate-limit headers.
- `runtime::resilience`: circuit breaker, retry policy, health state, fallback, load shedding.
- `runtime::store`: in-memory state store first; Redis-compatible trait later.
- `observability::access_log`: structured and redacted access logs.
- `observability::metrics`: Prometheus-compatible metrics registry and endpoint.
- `observability::tracing`: traceparent propagation and OpenTelemetry-ready span helpers.

## Phase 1 Scope

Phase 1 is the first implementation target.

### Observability

- Structured JSON access logs.
- Metrics endpoint, default `/metrics`, disabled by config when embedded callers do not want it.
- Low-cardinality metrics for request count, latency, upstream count/latency, rate limit hits,
  body rejections, and in-flight requests.
- Trace context propagation through `traceparent`.

### Security

- Host allowlist.
- Trusted proxy CIDR policy for inbound forwarded headers.
- Security response headers.
- Method guard.
- Header count and header byte limits.
- Credential-header rejection for configured anonymous/auth bootstrap routes.
- WAF-lite monitor/block mode for path/query/header heuristics.

### Traffic Control

- In-memory token bucket limiter.
- Rule match by route prefix, route pattern, service id, method, and surface.
- Keys: global, client IP, request header, route, dependency service.
- `429 application/problem+json`.
- `Retry-After`, `RateLimit`, `RateLimit-Policy`, and optional `X-RateLimit-*`.
- Global and per-upstream concurrency limits.

### Config And Docs

- Update `sdkwork.app.config.json` env bindings.
- Update `specs/component.spec.json` runtime behavior.
- Update config TOML examples.
- Update README.

## Phase 2 Scope

- Redis-backed distributed rate limiting and concurrency state.
- Circuit breaker per upstream and route.
- Retry with jitter for safe/idempotent methods.
- Active/passive health state.
- Fallback responses and stale-cache-for-GET hooks.
- Load shedding by global in-flight and upstream saturation.

## Phase 3 Scope

- Request streaming with bounded reader.
- Response compression.
- Response cache.
- Policy dry-run mode.
- Admin diagnostics endpoint, disabled by default.
- Policy inventory endpoint.
- Benchmark harness and route matcher optimization.

## Non-Goals

- No external Envoy/Kong/APISIX runtime dependency in Phase 1.
- No WASM/Lua/plugin runtime in Phase 1.
- No appbase IAM handler copy.
- No business authorization replacement in the gateway.
- No Redis requirement for embedded/local/test mode.

## Acceptance Criteria

- Every new behavior has a failing test first.
- All existing public router builders continue to compile.
- `cargo fmt --all -- --check` passes.
- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo build --workspace` passes.
- JSON config/spec files parse.
- Binary smoke covers `/healthz`, `/readyz`, and `/metrics` when enabled.


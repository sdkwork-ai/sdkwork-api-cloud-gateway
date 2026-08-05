# SDKWork API Gateway PRD

Status: active
Owner: SDKWork maintainers
Application: sdkwork-api-cloud-gateway
Updated: 2026-08-05
Specs: REQUIREMENTS_SPEC.md, DOCUMENTATION_SPEC.md, APPLICATION_GATEWAY_SPEC.md

## Document Map

- Add `PRD-<topic>.md` shards in this directory when the PRD grows beyond one reviewable screen.

## 1. Background And Problem

SDKWork operates a growing family of API applications (IAM, drive, commerce, IM, CMS,
forum, community, and more). Each application owns its domain APIs, but production
topologies need a single controlled ingress that provides:

- uniform security enforcement (headers, WAF, host allowlist, credential policy),
- traffic control (rate limits, concurrency limits, body limits),
- observability (access/audit logs, metrics, request tracing),
- policy governance (method allowlists, per-route limits),
- a single OpenAPI inventory and health/readiness surface.

Without a platform gateway, every application repeats security and traffic logic,
operational policy diverges, and clients must track per-service endpoints.

## 2. Target Users

| User | Need |
| --- | --- |
| End-user clients (app/PC/H5) | One stable origin for all `app`/`open` API surfaces with uniform errors |
| Backend operators (admin consoles) | One controlled `backend` surface with audit and rate limiting |
| Platform engineers | Embedded dependency composition without per-service base URLs |
| Security/ops teams | Centralized WAF, CSRF, credential policy, and observability |

## 3. Goals And Non-Goals

### Goals

- Serve the complete declared dependency graph from one Axum process (embedded
  composition, no per-dependency base URLs or split modes).
- Enforce security, traffic, and policy controls before dependency delegation.
- Provide `/healthz`, `/livez`, `/readyz`, `/metrics`, and `/openapi/*` surfaces.
- Support `standalone` (single node/edge) and `cloud` (multi-service production)
  deployment profiles from the same binary.
- Fail closed on missing dependency surfaces (honest 503), never silently fake success.
- Keep dependency API ownership in the owning application; the gateway composes only.

### Non-Goals

- Reimplement dependency-owned routes (IAM auth/oauth/iam, drive, commerce, ...).
- Hand-edit generated SDK output or generated OpenAPI copies.
- Own domain tables or migrations (the gateway bootstraps the process-shared pool).
- Provide per-dependency base URL routing or split-process modes.

## 4. Scope

### In scope

- Gateway-owned HTTP surfaces and OpenAPI inventory.
- Security pipeline: headers, host allowlist, trusted-proxy handling, CSRF,
  credential-header policy, WAF (query/header/JSON body), request size limits.
- Traffic pipeline: token-bucket rate limits, per-rule concurrency limits,
  global in-flight cap, bounded rate-limit memory.
- Policy pipeline: per-route method allowlists and max body bytes (dry-run support).
- Observability: sampled access/audit logs, Prometheus metrics, request spans.
- Embedded dependency composition for wired workspaces with exact route registry.

### Out of scope (dependency-owned)

- All `/app/v3/api/*`, `/backend/v3/api/*`, and open API route implementations.
- IAM authentication/authorization semantics beyond gateway-level controls.
- Database schema and migrations of embedded modules.

## 5. User Scenarios

1. A mobile client calls `/app/v3/api/auth/login`: the gateway enforces headers/WAF,
   delegates to the embedded IAM assembly, and returns the dependency response with
   rate-limit headers.
2. An attacker floods a login endpoint with synthetic `X-Forwarded-For` values: the
   gateway ignores forwarded headers unless trusted proxies are configured, so the
   rate limit cannot be spoofed.
3. A declared-but-not-embedded dependency surface is requested: the gateway returns
   503 `dependency_not_available` with an audit event instead of a generic 404.
4. An operator enables `csrfGuardEnabled`: state-changing requests with session
   cookies must prove same-origin or they are rejected with 403.
5. Prometheus scrapes `/metrics`: sharded counters are aggregated into the
   `sdkwork_api_cloud_gateway_*` series.

## 6. Success Metrics

| Metric | Target |
| --- | --- |
| Time to first byte for delegated requests | < 5 ms p95 in-process |
| Rate-limit memory bound | ≤ 100k bucket identities (~10 MB) per instance |
| Request body buffering | streaming; no full-body allocation for uploads |
| Metric record path | sharded, no single global lock |
| Startup with wired workspaces | no dependency on unwired surfaces |
| Standards gates | `check-pagination`, `check-api-response-envelope`, `check-api-operation-patterns` pass |

## 7. Phases

| Phase | Scope | Status |
| --- | --- | --- |
| 1 | Workspace build integrity, sqlx 0.9 alignment, all-features compile | done |
| 2 | Security controls: trusted proxy, CSRF, credential policy, JSON WAF, tracing | done |
| 3 | Traffic: bounded rate-limit memory, streaming body limits, sharded metrics | done |
| 4 | Declaration/execution convergence: wired workspaces + honest 503 for the rest | done |
| 5 | Pagination and API contract alignment across embedded dependencies | in progress |
| 6 | Full verification, packaging, release readiness | pending |

## 8. Linked Requirements

- `APPLICATION_GATEWAY_SPEC.md` — embedded dependency composition and routing.
- `SECURITY_SPEC.md` — security headers, WAF, rate limits, CORS, credential forwarding.
- `OBSERVABILITY_SPEC.md` — access/audit logs, metrics, trace propagation.
- `DATABASE_SPEC_PROCESS_SHARED_POOL.md` — one canonical process pool.
- `PAGINATION_SPEC.md` — canonical list/search pagination for embedded surfaces.
- `specs/component.spec.json` — dependency surface and integration contract.

## 9. Open Questions

- Distributed (cross-instance) rate limiting: introduce a Redis-backed store for
  multi-replica cloud deployments (tracked in Phase 6).
- Full OpenTelemetry trace export beyond request spans (tracked in Phase 6).

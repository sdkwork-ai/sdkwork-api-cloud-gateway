# Dynamic CORS policy and multi-level sub-domain origins

Status: implemented in `sdkwork-api-cloud-gateway` 0.1.72+ (runtime read path).
Audience: platform operators and the console team wiring the admin UI.

## Symptom

Browsers on `https://mall.sdkwork.com` (and any other first-party sub-domain that
is not in the exact allowlist) received:

```json
{
  "type": "https://docs.sdkwork.com/problems/40301",
  "title": "Permission required",
  "status": 403,
  "detail": "CORS origin is not allowed by API policy",
  "instance": "GET /app/v3/api/catalog/spus",
  "code": 40301,
  "failedStage": "cors",
  "reason": "forbidden"
}
```

Evidence from the production gateway log (`docker logs
sdkwork-api-cloud-gateway-production-i1-gateway-1`):

- `OPTIONS /app/v3/api/catalog/spus` returned `200` **with**
  `access-control-allow-origin: https://mall.sdkwork.com`, because the edge
  `CorsLayer` did honor the sub-domain rule (`SDKWORK_CORS_SUBDOMAIN_BASE_DOMAINS`);
- the follow-up `GET` returned `403` with `failedStage: cors` and no
  `access-control-allow-origin`, which is what the browser surfaces as a CORS
  failure.

## Root cause

The platform edge and the embedded request-path pipeline evaluated **different**
CORS policies.

1. `web_framework.rs::gateway_cors_policy` attached the registered console-host
   pattern but evaluated the sub-domain rule (`SDKWORK_CORS_SUBDOMAIN_BASE_DOMAINS`)
   only inside the edge layer predicate.
2. The embedded framework host builds its pipeline policy in
   `sdkwork-iam-web-adapter::iam_web_security_policy`, which projected **console
   hosts only**. Its Cors stage therefore rejected every origin outside the
   exact `SDKWORK_CORS_ALLOWED_ORIGINS` list.
3. `mall.sdkwork.com` is not an exact origin and not a console label, so the
   pipeline answered `40301`. `platform.birdcoder.com` worked only because it is
   listed exactly.

The same gap made deeper names (`region.console.sdkwork.com`) impossible to
declare inside the ordinary allowlist.

## Second defect uncovered behind the CORS failure

Because the Cors stage runs before route resolution, the `40301` masked a second
production defect. Probing the live gateway with a currently allowed origin
(`Origin: https://platform.birdcoder.com`) shows:

| request | result |
| --- | --- |
| `GET /app/v3/api/catalog/categories` | `200` |
| `GET /app/v3/api/promotions/offers` | `200` |
| `GET /app/v3/api/catalog/products` | `503` (dependency unavailable) |
| `GET /app/v3/api/catalog/spus` | `404 route is not registered in the gateway route manifest` |

The mall storefront (PC/H5/mini program/Flutter) calls
`GET /app/v3/api/catalog/spus` and `/catalog/spus/{spuId}` through
`sdkwork-commerce-app-sdk`, but no T1 domain service publishes those routes —
`sdkwork-mall/docs/architecture/api/storefront-browse-regression.md` already
records the same finding. The gateway is behaving correctly by returning
`40401`; the missing publisher belongs to the catalog/commerce contract, not to
this gateway. Fixing it requires publishing `catalog.spus.list` /
`catalog.spus.retrieve` (or moving the storefront to the published `products`
contract) in the owning application.

After the CORS fix, mall category and offer flows work; product list/detail
still needs that contract decision.

## Fix

- `sdkwork-web-core`:
  - `CorsPolicy` gained `registered_subdomain_hosts`; `allows_origin` now
    consults it, so every caller (edge layer, header writer, pipeline, admin
    validation) shares one answer.
  - New `cors_wildcard` module adds host-wildcard allowlist entries:
    - `https://*.example.com` — exactly one label;
    - `https://**.example.com` — one or more labels (second, third, fourth and
      deeper levels).
    Matching is suffix-anchored on whole DNS labels, scheme-exact, port-free,
    userinfo-free, requires a base domain of at least two labels, and never
    matches the apex.
- `sdkwork-web-bootstrap`: `with_registered_subdomain_hosts_from_env` plus the
  sub-domain vocabulary re-exports.
- `sdkwork-iam-web-adapter::iam_web_security_policy`: projects the sub-domain
  rule into the pipeline policy (fail-closed at startup), exactly like the
  console-host pattern.
- `sdkwork-api-cloud-gateway`:
  - `gateway_cors_policy` attaches the sub-domain rule to the policy;
  - config validation accepts host-wildcard entries in `cors.allowedOrigins`
    (production included);
  - a new `cors_dynamic` module provides the admin-managed overlay.

## Admin-managed (backend-configured) CORS

The platform edge reads the existing framework table and hot-swaps its
in-memory policy. No code change, no config edit and no redeploy is needed to
add or remove an origin.

### Storage

`framework_cors_policy` (created by `sdkwork-web-store-sqlx` migrations 004 and
014), one row per `(tenant_id, environment)`:

| column | meaning |
| --- | --- |
| `tenant_id` | deployment scope, default `0` (config `cors.dynamic.tenantId`; production `100001`) |
| `environment` | framework label `prod`/`test`/`dev`; the lifecycle names `production`/`staging`/`demo` are accepted as aliases |
| `allow_all_origins` | `0`/`1`; ignored in production-like environments |
| `allowed_origins` | JSON array of exact origins and/or host wildcards |
| `allow_credentials` | `0`/`1`, applied to the pipeline and header writer |
| `version` | change signal; the upsert increments it |

Example:

```json
{
  "allowAllOrigins": false,
  "allowedOrigins": [
    "https://platform.birdcoder.com",
    "https://**.sdkwork.com",
    "https://*.birdcoder.com"
  ],
  "allowCredentials": true
}
```

### Refresh model

- One snapshot (`Arc<CorsPolicySnapshot>`) per process, published atomically;
  the request path performs one read lock plus one `Arc` clone.
- A background scan (default every 10 s, config
  `cors.dynamic.refreshIntervalSeconds`, bounded 2–3600 s) reads the row and
  swaps the snapshot **only** when `version` changes, so the steady state costs
  one indexed point read per interval.
- Every process and replica runs its own scan; a fleet restart is jittered by up
  to 25 % of the interval. This is what keeps a multi-process/distributed
  deployment converged without a shared pub/sub channel and without restarts.
- A missing table, a decode error or a database outage keeps the last good
  snapshot (the static floor on first load); CORS never fails open.
- Invalid admin entries (non-HTTP schemes, credentials, paths, `*`, malformed
  wildcards) are logged and skipped.
- `allow_all_origins` from the row is ignored in production-like environments
  with a warning; use explicit origins or host wildcards instead.
- The overlay is handed to the embedded pipeline through
  `DynamicCorsPolicySource`; when the row is empty the source returns `None`, so
  the pipeline keeps its zero-copy static policy reference.

### Observability

- `GET /internal/gateway/diagnostics` now reports a `cors` block
  (`appliedVersion`, `dynamicOriginCount`, `refreshAttempts`, `refreshFailures`,
  `registeredSubdomainHosts`, …).
- `GET /metrics` exposes
  `sdkwork_api_cloud_gateway_cors_policy_refresh_total`,
  `..._refresh_failures_total`, `..._version`, `..._dynamic_origins`.

### Configuration

```toml
[cors.dynamic]
enabled = true
refreshIntervalSeconds = 10
tenantId = "0"
```

Environment overrides: `SDKWORK_CORS_DYNAMIC_ENABLED`,
`SDKWORK_CORS_DYNAMIC_REFRESH_INTERVAL_SECONDS`,
`SDKWORK_CORS_DYNAMIC_TENANT_ID`.

## Operator actions

- Add a first-party sub-domain family without a deploy: edit
  `/admin/settings` → 系统配置 → CORS 跨域设置 (or `PATCH` the settings document
  directly) and the edge applies it within one scan interval.
- Prefer `https://**.sdkwork.com` over enumerating hosts; it covers second,
  third and fourth levels.
- Remove an origin by removing it from `allowedOrigins`; the settings upsert
  bumps `version`, which every process observes on its next scan.

## Admin console surface (`/admin/settings` → 系统配置 → CORS 跨域设置)

The platform edge serves the settings document the console edits:

| method | path | purpose |
| --- | --- | --- |
| `GET` | `/backend/v3/api/system/cors/settings` | read the platform-scoped row (defaults when absent) |
| `PATCH` | `/backend/v3/api/system/cors/settings` | validate and upsert the row, bumping `version` |

- Auth: the composed pipeline authenticates the console's dual token and
  resolves the tenant; the handler refuses any tenant other than the configured
  platform scope (`cors.dynamic.tenantId`, production `100001`), so a tenant
  administrator cannot widen the platform edge.
- Body: `{ "allowAllOrigins": false, "allowedOrigins": ["https://**.sdkwork.com"],
  "allowCredentials": true }`. Entries are validated (HTTP(S) origin or host
  wildcard, no credentials/path/query/`*`), de-duplicated, capped at 512, and
  `allowAllOrigins` is rejected in production-like environments.
- Response: `SdkWorkApiResponse` `{ code: 0, data: { item: { allowAllOrigins,
  allowedOrigins, allowCredentials, version } }, traceId }`; `version` is a
  decimal string per `API_SPEC` §13.
- Console: `sdkwork-cloudrouter-pc` renders the "CORS 跨域设置" panel at the top
  of `/admin/settings` (origin list editor with wildcard hints, credentials
  switch, save/reload, version display).

Mounted only by the embedded runtime that has a PostgreSQL pool; it is part of
the composed pipeline (route manifest contribution), so the console reaches it
through the same authenticated edge as the other `/backend/v3/api/system/*`
settings endpoints.

## Remaining integration notes

- The framework control-plane route set
  (`/backend/v3/api/web-framework/cors_policies`) remains unmounted because its
  `web-framework.*` permissions are not seeded in the platform IAM catalog; the
  dedicated `/system/cors/settings` document above is the operator-facing
  surface instead.
- The live deployment picks up the new routes and console build on the next
  ordinary release. Server-side `env/<environment>.env` files created before
  this change keep their contents (`prepare-envs.sh` never overwrites them), so
  add `GATEWAY_CORS_DYNAMIC_TENANT_ID=100001` there (or re-run with
  `--force`) when deploying to an existing install.


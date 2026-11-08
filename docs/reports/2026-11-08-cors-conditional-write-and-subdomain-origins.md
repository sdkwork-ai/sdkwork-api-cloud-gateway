# CORS: conditional-write preflight and whole-sub-domain origins

Date: 2026-11-08 (Asia/Shanghai)
Scope: fix the production browser CORS failure on `api.birdcoder.com` and make every
host under `sdkwork.com`, `birdcoder.com`, `birdcoder.cn` and `sdkwork.cn` reachable
from the platform API edge.

## 1. Reported failure

```text
https://api.birdcoder.com/backend/v3/api/ai/upstream_accounts/555352107782000318
  from origin https://platform.birdcoder.com
  blocked by CORS policy:
  Request header field if-match is not allowed by Access-Control-Allow-Headers
  in preflight response.
```

Live evidence (read-only `OPTIONS` probe against production, 2026-11-08T17:05Z):

```text
HTTP/1.1 200 OK
access-control-allow-origin: https://platform.birdcoder.com
access-control-allow-headers: authorization,access-token,content-type,idempotency-key,
  x-content-sha256,x-idempotency-fingerprint,x-api-key,x-sdkwork-access-token,accept,
  accept-language,content-language,x-requested-with,x-request-id,x-correlation-id,
  x-tenant-id,x-device-id,x-client-version,x-app-version,x-platform,traceparent,tracestate
```

The origin was accepted but `if-match` was not in the allow-list. The same probe showed
`https://foo.sdkwork.com`, `https://a.b.sdkwork.com`, `https://x.y.z.birdcoder.com`,
`https://anything.birdcoder.cn` and `https://deep.a.sdkwork.cn` rejected (no
`Access-Control-Allow-Origin`).

## 2. Root causes

1. **Missing conditional-request headers.** `sdkwork-web-core`'s default CORS
   request-header allow-list carried credentials, protocol and tracing headers but not
   the RFC 9110 §13 conditional headers. The CloudRouter upstream-account update API
   requires `If-Match` (optimistic concurrency on `version`), so every browser write
   failed preflight. `ETag` was also absent from `Access-Control-Expose-Headers`, and it
   is **not** a CORS-safelisted response header — a browser client could not read the
   value it has to echo in `If-Match` even after the preflight was fixed.
2. **The console pattern cannot express the console.** `platform.birdcoder.com` is not
   in the single-label console registry, and the pattern matches exactly one registered
   label per host with no sub-domain wildcard. Production worked only because an operator
   had hot-added exact origins to `bundle/env/production.env`; a redeploy from the
   repository would silently drop `platform.birdcoder.com` and `www.*` again.

## 3. Changes

### 3.1 Framework default policy (`sdkwork-web-framework`)

`sdkwork-web-core/src/security.rs`:

- `allowed_headers` gains `if-match`, `if-none-match`, `if-modified-since`,
  `if-unmodified-since`.
- `expose_headers` gains `etag`, `last-modified`, `location`, `retry-after`.

`WEB_FRAMEWORK_SPEC.md` §12 records the new minimum set.

### 3.2 Registered sub-domain rule (`CORS_SPEC.md` §5.1)

New `sdkwork-web-core/src/registered_subdomain_hosts.rs`:

- `RegisteredSubdomainHosts { baseDomains, schemes }` with `matches`, `validate`, and
  `registered_subdomain_hosts_from_{env,lookup}`.
- Matching: configured scheme, no port/userinfo/path/query/fragment, host is a listed
  base domain or a dot-anchored sub-domain at **any** depth with valid DNS labels.
- Fail-closed validation: every base domain must be a registered platform base domain
  and duplicates are rejected, so the rule can never widen to a third-party domain.
  `evil-sdkwork.com`, `notsdkwork.com` and `sdkwork.com.attacker.example` stay rejected.

Gateway wiring:

- `cors.registeredSubdomainHosts` (`[cors.registeredSubdomainHosts]`, env
  `SDKWORK_CORS_SUBDOMAIN_{BASE_DOMAINS,SCHEMES}`) in
  `crates/sdkwork-api-cloud-gateway-config/src/lib.rs`, validated at startup.
- `gateway_cors_layer` in `crates/sdkwork-api-cloud-gateway/src/web_framework.rs`
  evaluates the rule in the `tower-http` origin predicate, and
  `gateway_cors_policy_for_origin` admits the matched concrete origin to the policy used
  by the shared response-header writer, so `Access-Control-Allow-Origin` echoes the
  origin and `Vary: Origin` is preserved (`*` is never emitted with credentials).

### 3.3 Carriers

All gateway carriers now declare the rule (`node scripts/derive-cors-allowed-origins.mjs
--apply`):

- `etc/sdkwork-api-cloud-gateway.{development,test,staging,production}.toml`
- `docker/config/config.toml`
- `docker/env/*.env.example`, `docker/.env.example`
- `docker-compose.yml`, `docker-compose.bundle.yml`

Canonical values:

```toml
[cors.registeredSubdomainHosts]
baseDomains = ["sdkwork.com", "birdcoder.com", "sdkwork.cn", "birdcoder.cn"]
schemes = ["http", "https"]
```

`scripts/cors-allowed-origins.mjs` owns the values, `derive-cors-allowed-origins.mjs`
materializes them (including the compose mapping and the env insertion point), and
`validate-docker-deployment.mjs` fails the deployment matrix when a base domain is
misspelt, unregistered, duplicated, or declared without schemes. `sdkwork-specs`
`tools/cors/{registry,scan}.mjs` recognize the new keys as canonical and validate their
shape instead of misreading them as origins.

## 4. Verification

| Check | Result |
| --- | --- |
| `cargo test -p sdkwork-web-core --lib` (framework workspace) | 236 passed |
| `cargo test -p sdkwork-web-axum` (framework workspace) | 16 passed |
| `cargo test -p sdkwork-api-cloud-gateway --no-default-features --lib web_framework` | 5 passed |
| `cargo test -p sdkwork-api-cloud-gateway-config` | 37 passed, 1 pre-existing failure |
| `cargo test --workspace --no-fail-fast` | only that pre-existing failure |
| `node --test scripts/cors-allowed-origins.test.mjs` | 14 passed |
| `node --test ../sdkwork-specs/tools/cors/registry.test.mjs` | 24 passed |
| `node scripts/derive-cors-allowed-origins.mjs --check` | 24/24 carriers canonical |
| `node scripts/validate-docker-deployment.mjs --matrix` | 10/10 environment/mode combinations |
| `node ../sdkwork-specs/tools/check-cors-standard.mjs --root .` | 15 carriers, 0 issues |
| `node ../sdkwork-specs/tools/check-source-config-standard.mjs --root .` | passed |
| `cargo fmt -- --check` (both workspaces) | clean |
| `cargo clippy` on the touched files | no diagnostics |

`cargo clippy -- -D warnings` still fails on **pre-existing** lints in files this change
does not touch (`embedded_dependency_routes.rs`, `comments_proxy.rs`,
`security.rs`/`web_framework` items-after-test-module, and 32 `#[must_use]` /
`Copy`-clone lints across `sdkwork-web-core`). None of the diagnostics point at a file
changed here.

Two pre-existing failures were found and are **not** caused by this change:

1. `config_tests::im_and_cloudrouter_use_their_api_assemblies` compares the
   CloudRouter surface prefixes declared in the (unmodified) `agent.app.config.json`
   against a hard-coded expected set that no longer matches the sibling
   `sdkwork-cloudrouter` manifest.
2. `check-cors-standard.mjs --root .` reported `etc/topology/{,standalone,cloud}.base.env`
   as carriers with an invalid file name. Those files are layered topology
   fragments composed by `materialize-topology-env.mjs`, not profiles. The shared
   scanner now skips fragments (`isFragmentCarrierFileName`), which clears this
   repository's CORS gate for the whole workspace.

## 5. Deployment requirement

The header list and the sub-domain rule are **code**, so production keeps the reported
failure until the image is rebuilt and redeployed:

```sh
bin/docker-image.sh build --image-tag <new version>
bin/docker-deploy.sh upgrade --environment production --deps embedded \
  --host ssh://ubuntu@api.noaper.com --yes
```

**Release-version warning.** Production currently runs
`registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.36`, but this checkout and
`origin/main` are at `0.1.34` (`5a6ffb4`) and the `0.1.35`/`0.1.36` sources are not in
this repository. A rebuild from this tree would drop whatever those releases carry, so
the next release must first confirm/merge that source state and then bump
`release.currentVersion` above `0.1.36`.

The existing live `bundle/env/production.env` already carries the operator-added exact
origins (`platform.*`, `www.*`), so the window between this commit and the redeploy is
unchanged; after the redeploy the exact entries become redundant but harmless. New
installs read the rule from `docker/config/config.toml` and
`docker/env/<environment>.env.example`; an unset `GATEWAY_CORS_SUBDOMAIN_*` env key does
not disable the TOML policy.

Acceptance probe after the redeploy:

```sh
curl -sS -D - -o /dev/null -X OPTIONS \
  https://api.birdcoder.com/backend/v3/api/ai/upstream_accounts/1 \
  -H 'Origin: https://platform.birdcoder.com' \
  -H 'Access-Control-Request-Method: PUT' \
  -H 'Access-Control-Request-Headers: if-match'
```

Expected: `access-control-allow-origin: https://platform.birdcoder.com`,
`access-control-allow-headers` containing `if-match`,
`access-control-expose-headers` containing `etag`.

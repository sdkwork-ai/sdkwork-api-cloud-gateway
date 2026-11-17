# 2026-11-18 — Caller-identity propagation for registry-resolved backend SDK calls

`APP_SDK_INTEGRATION_SPEC.md` §5.2.1 already required every user-scoped backend call to carry the
caller's dual tokens (`Authorization: Bearer <auth token>` + `Access-Token`) or the typed
`WebRequestContext`, and forbade substituting process credentials. It had **no** implementation
carrier and **no** verification, so registry-resolved calls (the default production path after
`DISCOVERY_SPEC.md` §12.1) could authenticate as the deployment, an API key, or nothing at all and
still pass every gate.

This change lands the shared carrier, the framework plumbing, and an enforcing ratchet, then fixes
the sites iteratively. Nothing was deployed.

## 1. Contract recap (§5.2.1 / §5.4)

- A user-scoped backend call carries **that user's** identity; process credentials are forbidden.
- `WebRequestContext` carries presence flags only — it is serializable and must never hold raw
  tokens. The dual tokens therefore need their own typed carrier.
- A generated client that stores per-call headers must be built per call, or cached by
  `(base URL, credential fingerprint)`; a pooled instance must never be mutated with another
  caller's credentials.
- A genuinely background/bootstrap-only path may use an explicitly declared service identity, never
  a borrowed end-user token.
- An identity failure must name the destination surface and distinguish "no caller identity was
  present" from "the destination rejected the identity".

## 2. What landed

### 2.1 `sdkwork-utils` — the shared carrier (commit `0cda7d7`)

`sdkwork_utils_rust::caller_identity`:

- `CallerCredentials` — normalized carrier built from a header lookup (trim, blank-is-absent,
  `Bearer` stripped) or from raw values; `wire_headers()` renders the exact outbound pair;
  `apply()` pushes it through a generated client's `set_header`; `apply_typed()` drives the typed
  setters **access token first, authorization bearer last**, matching generated-client header
  hygiene (`set_access_token` cleans up a stale API-key header, `set_auth_token` owns the bearer).
- `missing_caller_identity(surface)` — the canonical "no caller identity was present" diagnostic
  naming the destination surface.
- Six unit tests cover normalization, blank/scheme-only rejection, dual/access-only wire shapes,
  setter ordering, and the diagnostic.

It never synthesizes, widens, or caches a credential.

### 2.2 `sdkwork-web-framework` — framework plumbing (commit `e1f1ae3`)

- `inject_web_request_context_with_credentials` inserts `CallerCredentials` as a request extension
  beside `WebRequestContext` at the ContextInjection stage; `inject_web_request_context` keeps its
  signature and delegates with an empty carrier for existing hosts.
- The axum frame pipeline populates it from the verified `WebCallState.credentials`; the anonymous
  identity middleware populates it from inbound headers on public/open-api paths.
- The carrier is always present (empty when the request carried no credential) so an adapter can
  fail closed instead of silently substituting a service token. It is never serialized, audited, or
  copied into `WebRequestContext`.
- Re-exported from `sdkwork_web_core` (`CallerCredentials`, `CallerTokenKind`,
  `missing_caller_identity`) so handlers import one surface.
- Verified: `cargo test -p sdkwork-web-core --lib` (250 passed, one new injection test) and
  `cargo test -p sdkwork-web-axum` (13 passed).

### 2.3 `sdkwork-specs` — enforcing ratchet (commit `1b188dc`)

`check-http-registry-resolution.mjs` now also fails a registry-resolved SDK construction site that

- applies no caller identity (`CallerCredentials`, `set_auth_token`, `set_access_token`,
  `missing_caller_identity`), or
- reads a process `*ACCESS_TOKEN` / `*AUTH_TOKEN` environment value (directly or through a
  `const ..._ACCESS_TOKEN_ENV` indirection),

unless it declares `identity:service <justification>` for a declared background/bootstrap path.
Known debt is tracked in the checker with a per-site removal plan and reported as warnings — never a
silent pass, never an unrecorded new site. The infrastructure check also requires
`caller_identity.rs`. The spec section now states the carrier contract, the access-first wire order,
the per-call/credential-keyed client rule, and points its verification at this gate.

Workspace run: 6,268 Rust files, **19** registry-resolved sites, **12 aligned**, **7 tracked**.

## 3. Audit matrix (read-only, four parallel audits)

| # | Site | Destination / trigger | Today | Verdict |
|---|---|---|---|---|
| 1 | generations `provider-adapter/gateway.rs` | cloudrouter media, **user turns** | **caller pair** via `MediaSdkGateway::with_caller`; pool keyed by `(base URL, credential fingerprint)`; caller-less worker path keeps the declared service identity | **ALIGNED** (§7) |
| 2 | image `provider-adapter/adapter.rs` (+`embedded.rs`) | cloudrouter image, user turns + background poller | process API key (`X-Api-Key`); embedded variant sends the same key as a bare bearer | GAP |
| 3 | voice `embedded-bootstrap/bootstrap.rs` | cloudrouter speech, latent | process API key | GAP (unwired) |
| 4 | aiot `intelligence-bridge/cloud_router.rs` | cloudrouter ASR/TTS, **device turn** | process API key, single shared client for all sessions | GAP |
| 5 | knowledgebase `agent-provider/cloud_router.rs` | cloudrouter chat, **user turn** + background embedding | process `SDKWORK_CLOUDROUTER_API_KEY` | GAP (user turn) |
| 6 | canvas `api-canvas-assembly/drive_http_adapter.rs` | drive **app-api**, user writes | **caller pair** from `CanvasActorContext` through the Drive commands; per-call caller-scoped client; process token removed | **ALIGNED** (§8) |
| 7 | notes `api-notes-assembly/drive_http_adapter.rs` | drive **app-api**, user writes | **caller pair** from `NotesActorContext` through the Drive commands; per-call caller-scoped client; process token removed | **ALIGNED** (§9) |
| 8 | course `course-embedded-bootstrap/drive_sdk.rs` | drive **app-api**, user writes | **caller pair** from `CourseServiceContext`; per-call client with both headers; boot-time process token requirement removed | **ALIGNED** (§9) |
| 9 | deploy `deploy-web-port/lib.rs` | webserver **internal API**, background worker | `set_api_key` (ingress token in the wrong header) | SERVICE-IDENTITY-JUSTIFIED + **mechanism GAP** |
| 10 | deploy `identity-provisioning-port/http.rs` | IAM backend, control-plane | token in JSON body (`bootstrap-body`, headers forbidden) | SERVICE-IDENTITY-JUSTIFIED (matches contract) |
| 11 | deploy `drive-port/drive_http_adapter.rs` | drive app-api, user uploads | caller dual tokens, per call | **ALIGNED** (add empty-credential fail-fast) |
| 12 | deploy `content-provider-port/...` | drive app + drive/kb internal | app leg caller tokens; internal legs `X-Api-Key` | mixed: app ALIGNED, internal mechanism GAP |
| 13 | webserver `website_provider_http_adapter.rs` | drive/kb internal, anonymous data plane | `X-Api-Key`, pooled at bootstrap | SERVICE-IDENTITY-JUSTIFIED + mechanism GAP |
| 14 | webserver `website_runtime_cloud.rs` | webserver internal, background node | `X-Api-Key`, shared client | SERVICE-IDENTITY-JUSTIFIED + mechanism GAP |
| 15 | webserver `webserver-agent/main.rs` | webserver backend agent routes, background | `X-SDKWork-Agent-Token` only; route also requires `Access-Token` | SERVICE-IDENTITY-JUSTIFIED + mechanism GAP |
| 16–19 | agents memory/knowledge bridges, cloudrouter tool client/executor, knowledgebase memory | memory/knowledge/cloudrouter | caller dual tokens, per call, fail-closed (bridges and media tool set auth before access) | **ALIGNED** (ordering deviations; see §4) |

## 4. Cross-cutting findings

1. **The dominant remaining defect is mechanical, not identity-model:** internal/agent generated
   SDK clients send the open-api api-key channel (`X-API-Key`) to `ingress_token` / `agent_token`
   surfaces that require `X-SDKWork-Ingress-Token` (or `X-SDKWork-Agent-Token`) **plus**
   `Access-Token`, and the internal OpenAPIs declare `security: [{ApiKey}]` while their own
   `x-sdkwork-route-auth` says `ingress-token`. This breaks the deploy web port, the content
   provider's internal legs, the website data plane, the website runtime pull, and the node agent
   heartbeat. It needs one contract decision (machine ``Access-Token`` for internal/agent profiles,
   and the SDK scheme name) plus per-site header fixes.
2. **Pooling must become credential-aware** at the image adapter before it adopts caller tokens (it
   still keys only by base URL and `clear()`s at 16). The generations media pool is now keyed by
   `(base URL, credential fingerprint)`; the agents media client is per-call and already safe.
3. **Ordering deviations** in the agents memory/knowledge bridges and media tool client
   (auth before access) are latent, not credential loss: today's generated clients keep both
   headers regardless. Adopting `CallerCredentials::apply_typed` aligns them anyway.
4. **Streaming diverges from non-streaming**: `agents-tool-cloudrouter`'s `chat_stream` selects the
   deployment API key whenever it is configured, while the same turn non-streamed authenticates as
   the caller — a user turn must not gain a service identity.
5. **The Drive app-api adapters shipped without the dual token.** canvas/notes/course sent only
   `Access-Token` from a process token and never `Authorization`, so those split
   deployments failed `missing-auth-token`; all three are now migrated (§8/§9).
6. **Grounding swallows identity failures** (`context_grounding.rs` logs a warning and returns empty
   snippets), so neither "no caller identity" nor "destination rejected" reaches an operator-visible
   problem detail. §5.2.1's diagnostic requirement is met for media/rig, partial for grounding.

## 5. Remaining plan (tracked by the gate)

Order by user impact:

1. ~~**generations media gateway**~~ — **done** (see §7).
2. ~~**canvas** (§8), **notes** (§9), **course** (§9)~~ — done. Remaining in this family: none.
4. **image provider** — thread the caller pair to the provider boundary; background poller keeps the
   declared key; normalize the embedded bearer/api-key divergence.
5. **internal/agent credential mechanism** — decide and implement the ingress/agent wire contract
   (see §4.1), then fix the five sites and pin each with a loopback header-capture test.
6. **aiot device turn / voice (when wired)** — resolve device → tenant/organization/user at the
   handshake and carry it into the speech command.
7. **deploy drive-port / content-provider app leg** — fail fast with `missing_caller_identity`
   instead of dispatching without a credential; **grounding** — surface the destination and the
   absent-vs-rejected distinction.

Each fix removes its `IDENTITY_BASELINE` entry in `check-http-registry-resolution.mjs` in the same
change; the gate fails any new site and any new process-token substitution in the meantime.

## 6. Verification

- `node sdkwork-specs/tools/check-http-registry-resolution.mjs --workspace .` — passes with 8 tracked sites and per-site removal plans (11 aligned; fails on any new violation).
- `cargo test -p sdkwork-utils-rust --lib caller_identity` — 8 passed; `cargo test -p sdkwork-web-core --lib caller_scope` — 2 passed (scope isolation and no spawn inheritance).
- `cargo test -p sdkwork-web-core --lib` — 250 passed; `cargo test -p sdkwork-web-axum` — 13 passed.
- `cargo test -p sdkwork-generations-provider-adapter` — 70 lib tests + the registry integration test
  pass; `cargo check --workspace` in `sdkwork-generations` passes.
- `cargo fmt -- --check` in `sdkwork-utils` and `sdkwork-web-framework`; the touched generations files
  are rustfmt-clean (the repository has pre-existing formatting drift in
  `sdkwork-api-generations-assembly/src/bootstrap.rs`, untouched here).

Blocked, not caused by this change: the gateway binary re-check currently fails inside
`sdkwork-iam-web-adapter` (`unresolved import crate::application_scope` /
`crate::installation_scope_cache`) because another concurrent workspace session is mid-edit in
`sdkwork-iam` (two new modules on disk, `lib.rs` not yet updated). The framework change is additive
(`inject_web_request_context` signature unchanged) and both framework crates compile and pass their
suites.

## 7. Round 1 — generations media path (commit `9db459c`)

The generations provider adapter is the first site migrated end to end:

- `GenerationsRequestContext` / `GenerationsHttpRequestContext` carry `CallerCredentials`; the HTTP
  bootstrap injector reads the framework's caller-credentials extension and attaches the verified
  pair. The MCP/worker port builds the context through `from_parts` and stays `identity:service`.
- `MediaSdkGateway::with_caller` returns a handle bound to one caller. It keeps the resolved
  registry target and the shared pool, whose clients are keyed by
  `(base URL, credential fingerprint)`; the pool is bounded at 64 and each caller's client is never
  mutated with another caller's tokens.
- `media_credential_headers` sends only the caller pair on a caller-bound dispatch; the declared
  deployment identity (`X-Api-Key`, or the service token pair) is used only by a caller-less
  background/worker dispatch. This also fixed a production reality: no
  `GENERATIONS_MEDIA_GATEWAY_*` credential is configured on the running gateway, so user media
  calls previously carried no credential at all.
- Image, video, music, sound-effect and voice adapters bind the caller for `dispatch`; the scripted
  test double and the MCP port remain service-identity.

Tests: caller-only wire headers (no `X-Api-Key`), caller-less service identity, credential-keyed
pool separation, and an adapter-level test proving `bind_caller` rebinds the gateway with the
caller's fingerprint.

No build, tag, or deploy was performed.

## 8. Round 2 — canvas Drive path (commit `d63d4cb`)

The canvas HTTP Drive adapter was the worst instance of the defect: it read a
process `SDKWORK_ACCESS_TOKEN` once at bootstrap, set only `Access-Token`, and
never sent `Authorization` — while the Drive app-api requires the dual-token
pair, so every split-deployment page save failed `missing-auth-token`.

- `sdkwork_web_core::caller_scope` exposes the verified pair as a task-local
  installed around the handler by the framework layer (and reused by a composed
  module host from the request extension). It is a transport convenience, never
  an authorization decision, and is deliberately absent in spawned/blocking
  work.
- `CanvasActorContext` carries `CallerCredentials`; the HTTP auth builder fills
  it from the accessor, so no handler signature changes were needed while the
  typed context still flows explicitly through service and port boundaries.
- The five Drive commands carry the caller. The adapter resolves the registry
  target per call, builds a caller-scoped Drive app-api client with
  `set_auth_token` + `set_access_token`, and fails closed with
  `missing_caller_identity("sdkwork-drive app-api")` when the request carried
  none. Tenant/organization ids stay in the commands only for the local service
  contract; Drive resolves scope from the credential.
- Build-fix prerequisite: the Drive uploader port renamed
  `find_default_storage_provider` to `find_default_storage_target`, and two
  test-only dynamic SQL strings now carry `sqlx::AssertSqlSafe` after audit.

Next in this pattern: notes (same actor-context shape) and course
(`CourseServiceContext`).

## 9. Round 3 — notes and course Drive paths

Both split-deployment Drive adapters had the same defect as canvas (a startup
process `SDKWORK_ACCESS_TOKEN`, `Access-Token` only, no `Authorization`, ignored
tenant context) and were migrated with the same shape:

- notes: `NotesActorContext.caller` filled from the request-scoped accessor,
  carried by the five Drive commands, per-call caller-scoped client,
  fail-closed diagnostic. Commit `2bcbb33`.
- course: `CourseServiceContext.caller` (`#[serde(skip)]`) filled by the HTTP
  auth builder, the port methods already receive the context, and the per-call
  client now applies both tokens; the boot-time token requirement and its
  constant are gone (`8979761`). The compatibility context mappers in both route
  layers default to an empty carrier and are unused.

Both needed the same build prerequisites as canvas (Drive uploader
`find_default_storage_target` rename, `sqlx::AssertSqlSafe` in test-only dynamic
SQL); notes was otherwise unbuildable against the current workspace.

Verified for notes: `cargo test --workspace` 22/56/9/2 pass (the same
pre-existing web-core route-manifest assertion is the only failure). Verified
for course: `cargo test --workspace` 25/26 pass, including the new fail-closed
caller test and the updated source guard (no process token, diagnostic present).

Gate after this round: 19 registry-resolved sites, **11 aligned**, **8 tracked**.

## 10. Round 4 — knowledgebase chat turn

The knowledge agent chat turn handed the kernel a cloud-router client built from
the deployment API key, so a user's question was answered and billed as the
deployment. Both the caller pair and the capability-specific credentials already
reached the chat service; only the LLM client ignored them.

- `resolve_cloud_router_client(caller)` composes the per-turn headers from a
  pure, unit-tested helper: the caller's `Authorization`/`Access-Token` on a user
  turn (never mixed with the deployment key), `X-Api-Key` only on the caller-less
  background path, and nothing when neither is configured.
- The chat host builds that client from `KnowledgeAgentChatCredentials` before
  constructing the provider, so the identity survives the blocking kernel
  invocation — the client carries its headers, which a request-scoped task-local
  would not (deliberate: spawned/blocking work never inherits a caller).
- The embedding/retrieval backend keeps `resolve_cloud_router_client_from_env()`,
  the declared service identity for background work.

Verified: `cargo test -p sdkwork-knowledgebase-agent-provider` (31 lib tests plus
the two new identity tests); the app-api route crate compiles.

### Image provider prerequisite discovery

The image provider adapter cannot be migrated yet: its committed source no
longer compiles against the current generated `cloudrouter_open_sdk` (the
midjourney/nano-banana models and client accessors are gone, the OpenAI image
request gained fields, and `ViduReferenceToImageRequest` lost `style`). That is
unrelated cross-repository drift, not a caller-identity defect; the migration was
attempted, reverted unverified, and is now recorded as blocked on reconciling the
adapter with the regenerated SDK.

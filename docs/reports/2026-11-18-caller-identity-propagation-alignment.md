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

Workspace run: 6,268 Rust files, **19** registry-resolved call sites, **19 aligned**, **0 tracked**.

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

## 11. Round 5 — agents streaming precedence and the machine-credential wall

### Streaming/non-streaming divergence fixed

`HopCredentials::select` preferred a configured `SDKWORK_CLOUDROUTER_OPEN_API_KEY`
over the caller's dual tokens, so a *streamed* user turn authenticated as the
deployment while the same turn non-streamed authenticated as the caller. The rule
(commit `3447623`, sdkwork-agents) is now: the caller pair wins whenever the turn
carries a non-blank auth token; the deployment key is used only for a caller-less
(background/worker) turn as its declared service identity; a turn with neither
presents no credential at all. Both transports share the rule and the parity test
now covers all four cases in both arms.

This does not change the gate count (the site already carried a caller pair); it
removes a principal divergence inside an aligned call.

### Why the internal/agent cluster cannot be fixed mechanically

The five remaining `identity:service`-shaped sites (deploy web runtime, content
provider internal legs, website provider data plane, website runtime pull, node
agent heartbeat) fail on a missing credential source, not on code style:

- the IAM runtime requires `X-SDKWork-Ingress-Token` / `X-SDKWork-Agent-Token`
  **and** `.with_reason("missing-access-token")` an `Access-Token` that resolves
  to an IAM application/tenant scope;
- the deployments are configured with an ingress or node token only; no
  application access token exists to send;
- the generated internal SDKs expose `set_api_key`, which renders `X-API-Key` —
  a header the ingress/agent credential profile explicitly rejects as
  contamination.

Resolving it therefore needs a provisioning decision (issue an IAM application
access token to each machine deployment) or a contract change (drop the
Access-Token requirement for machine surfaces). Both are `SECURITY_SPEC` /
`IAM_SPEC` level decisions, so the call sites are left untouched and the status
is recorded here for human review rather than half-fixed with a header swap that
would still fail 401.

Round status: 19 registry-resolved sites, **12 aligned**, **7 tracked** — the
tracked set is now `image` (blocked on cloudrouter SDK reconciliation), five
internal/agent sites (blocked on credential provisioning), and `aiot`
device turn (documented service identity requiring a device→user decision).

## 12. Round 6 — the machine service-identity helper and the deploy publisher

`APP_SDK_INTEGRATION_SPEC.md` §5.2.1 only names caller identity; the machine
surfaces need the complementary rule, now encoded in
`sdkwork_utils_rust::service_identity` (`d6bfd85`):

- internal-api authenticates `X-SDKWork-Ingress-Token` + `Access-Token`;
- backend-agent authenticates `X-SDKWork-Agent-Token` + `Access-Token`;
- `ServiceCredentials` carries the pair, refuses to render while incomplete, and
  `missing_service_credentials` names the surface and both settings.

The first site migrated with it is the deploy runtime-assignment publisher
(`87af1ea`): it no longer sends the ingress token as `X-API-Key`; it reads an
optional `SDKWORK_DEPLOY_WEB_INTERNAL_API_ACCESS_TOKEN_FILE` and renders the pair
on the profile headers. A half-configured deployment fails closed with the
actionable diagnostic instead of dispatching a request that the destination
reports as a generic 400/401. The setting is optional at boot (existing
deployments keep starting and surface the error at publication time) and is
documented in the topology env files, operator README and component specs.

The site declares `identity:service`: a background publisher with a declared
machine identity and no end-user request to borrow tokens from. Gate status after
this round: 19 sites, **13 aligned**, **6 tracked**.

## 13. Round 7 — webserver data-plane machine identity

The split website data plane had the same defect as the deploy publisher in
three places: the runtime-assignment pull, the Drive provider and the
Knowledgebase provider each sent a node/ingress token through the generated
client's `set_api_key` (i.e. `X-API-Key`) and never an `Access-Token`. All three
now render the internal-api pair through `sdkwork_utils_rust::service_identity`
and fail closed with a diagnostic naming both required settings
(`sdkwork-webserver` `e42fb26b` + `7eb61921`).

- New settings: `SDKWORK_WEBSERVER_NODE_ACCESS_TOKEN_FILE`,
  `SDKWORK_WEBSERVER_DRIVE_INTERNAL_API_ACCESS_TOKEN_FILE`,
  `SDKWORK_WEBSERVER_KNOWLEDGEBASE_INTERNAL_API_ACCESS_TOKEN_FILE`, documented in
  the data-plane env example, the Kubernetes manifest and three component specs.
- The provider bootstrap now preserves the diagnostic (`ProviderConfig(message)`)
  instead of collapsing every construction failure into a generic client error;
  the runtime cloud source gained a `Configuration` variant for the same reason.
- Both modules declare `identity:service` (node machine surface).
- The loopback wire test asserts the real headers: `x-sdkwork-ingress-token` and
  `access-token` on both the pull and the observation.

Verified in WSL (the crate is Unix-only): `cargo test -p
sdkwork-api-webserver-standalone-gateway --lib` → 471 passed. Source-config
standard gate passed.

Remaining tracked sites (4): content-provider internal legs, node agent
heartbeat, aiot device turn, image provider (blocked on cloudrouter SDK
reconciliation).

## 14. Round 8 — content provider and node daemon: the machine cluster is complete

Two more machine surfaces had the same shape and are migrated:

- **deploy content provider** (`a2eac0b`): the Drive publication and
  Knowledgebase wiki-publication Internal API legs sent ingress tokens as
  `X-API-Key` and never an `Access-Token`. Both now render the pair through
  `service_identity`; `from_env` requires the two new access-token files (the
  split deployment already fails at startup when its opt-in is incomplete);
  documented in the topology env files, Kubernetes manifest, operator README and
  five component specs; `identity:service` declared; two new tests cover the
  fail-closed diagnostic and the complete pair.
- **node daemon** (`3dcc92fe`): agent routes require
  `X-SDKWork-Agent-Token` **plus** an `Access-Token`, so every heartbeat and sync
  failed `missing-access-token`. The daemon now reads
  `SDKWORK_WEBSERVER_AGENT_ACCESS_TOKEN_FILE` at startup, validates it, and
  renders the pair (`ServiceTokenKind::Agent`); `identity:service` declared. The
  loopback heartbeat test asserts both headers, a new test proves the fail-closed
  builder, and the agent contract test asserts the service-identity pair instead
  of `set_agent_token` (and forbids `set_api_key`).

Verified: deploy content provider 19 tests, `cargo check` for the service host and
runtime-assignment worker, source config gate; node daemon 25 lib + 4 integration
tests (WSL), `node --test tests/contract/agent-sync-state.contract.test.mjs` 4
passed, source config gate.

Gate status: 19 sites, **16 aligned**, **3 tracked** — aiot device turn, voice
(unwired), image provider (blocked on cloudrouter SDK reconciliation). Every
remaining tracked site needs a product/credential decision or an upstream SDK
reconciliation, not another mechanical migration.

## 15. Round 9 — the gate now audits every registry call site, not only SDK callers

The checker used to require a generated-SDK constructor before it applied the
identity rule, so a **raw HTTP client** that resolved its target through the
registry was invisible. An independent sweep for `registry_base_url_or` found one:
`sdkwork-deploy-identity-provisioning-port` (the split IAM provisioning adapter).
It is not a defect — it authenticates with a dedicated IAM backend provisioning
token from a secret file, carried in the provisioning body — but the gate could
not see it, so a future regression there would have passed silently.

Changes (`sdkwork-specs` `318e72a`, `sdkwork-deployments` `97c1b8d`):

- the identity rule now keys on an actual resolution call
  (`registry_base_url_or` / `registry_base_url` / `registry_http_target`), so
  generated-SDK and raw-HTTP call sites are audited alike;
- a new rule rejects `set_api_key` at a registry call site unless the module
  declares `identity:service` — applying `X-Api-Key` is now a machine-checked
  substitution, not a code-review expectation;
- `APP_SDK_INTEGRATION_SPEC.md` gained §5.2.1.1: the two machine profiles, the
  `ServiceCredentials` helper, the fail-closed diagnostic, and the requirement to
  provision an IAM application access token (never an inline secret);
- the deploy IAM provisioning adapter declares `identity:service` with its
  justification.

Current matrix: 19 registry-resolved call sites, **16 aligned**, **3 tracked**
(aiot device turn, voice, image), each with a per-site removal plan. The
independent sweep also confirmed no unclassified site remains: every file that
resolves through the registry either carries a caller pair, declares
`identity:service`, or is tracked in the gate.

### The three remaining sites need a decision or an upstream fix

- **aiot device turn**: the device edge authenticates a device bearer and derives
  an association (tenant/organization) from the device credential repository; the
  Xiaozhi session retains no end-user dual token, and `SpeechTurnInput` carries no
  identity at all. Either the device session binds a user credential at pairing
  (then the caller pair can be forwarded), or the turn is declared a device-level
  service identity with explicit attribution — a product/security decision.
- **voice**: latent (no wired route); migrate with the `with_caller` shape when it
  is wired.
- **image**: the adapter no longer compiles against the regenerated
  `cloudrouter_open_sdk` (midjourney/nano-banana models and accessors removed,
  OpenAI image request reshaped). Reconcile first, then apply the §7 `with_caller`
  shape.

## 16. Round 10 — completion matrix and verification evidence

The strengthened gate plus an independent sweep (all files calling
`registry_base_url_or`) classify every call site. No site is unclassified except
the three tracked ones, each of which is blocked on a decision or an upstream
reconciliation rather than on code:

| Class | Sites |
|---|---|
| **Caller pair (user-scoped)** — `Authorization` + `Access-Token` from `CallerCredentials`, per-call client | agents cloudrouter client, agents memory bridge, agents knowledgebase bridge, canvas Drive adapter, course Drive adapter, deploy Drive adapter, knowledgebase agent-provider chat, knowledgebase memory app-api, notes Drive adapter |
| **Declared service identity** — `identity:service`, service-token header + `Access-Token` via `ServiceCredentials`, fail closed | deploy web publisher, deploy content provider (Drive + Knowledgebase legs), deploy IAM provisioning (dedicated provisioning token), generations media gateway (caller pair when present, declared identity otherwise), webserver website provider, webserver runtime pull, webserver node daemon |
| **Tracked** — removal plan in the gate | aiot device speech turn (device-session identity decision), voice (route not wired), image (cloudrouter SDK reconciliation) |
| **Infrastructure** | `sdkwork-utils` `service_base_url` (the resolver itself) |

Verification evidence collected across the migration:

- `node sdkwork-specs/tools/check-http-registry-resolution.mjs --workspace <root>`
  — 6,268 files, 19 call sites, **16 aligned, 3 tracked**, and now also fails a
  raw HTTP client or a `set_api_key` without `identity:service`.
- `sdkwork-utils`: `CallerCredentials` + `ServiceCredentials` unit tests
  (fingerprint, redaction, wire order, profile headers, fail-closed diagnostic).
- `sdkwork-web-framework`: `caller_scope` task-local tests (no spawn inheritance)
  and the ContextInjection credential carrier.
- generations 70 lib tests + registry integration; canvas 24 + 7 + 5; notes
  22/56/9/2; course 25/26; knowledgebase agent-provider 31; agents cloudrouter
  68 + integration; webserver gateway 471 (WSL); webserver daemon 25 + 4;
  deploy web port 6; deploy content provider 19.

The objective's code deliverables are complete for every site that can be
decided today: a shared caller-identity carrier, a shared machine
service-identity carrier, per-site migration with fail-closed diagnostics and
tenant/organization propagation, and a static gate that now covers every
registry target-resolution call site. The three tracked sites stay open in the
gate until their decisions land; the goal therefore remains active rather than
being marked complete.

## 17. Round 11 — the gate no longer tolerates a stale exemption

One more way the gate could silently pass a regression: a baseline entry is a
*current* state, not a permanent exemption. Once a site was migrated, its entry
stayed in the map and — if the site later regressed — the gate would have listed
it as "tracked" instead of failing. The same held for the base-URL baseline.

`sdkwork-specs` now fails the gate when:

- an identity-baseline file no longer exists, or already declares
  `identity:service` / a caller identity (the entry must be removed in the
  migration change);
- a base-URL-baseline file no longer exists, or now resolves through the registry.

Verified two ways: a deliberately stale entry makes the checker exit 1 and names
the entry, and the real workspace run still passes with 19 call sites, 16
aligned, 3 tracked.

At this point the migration has no silent exemption left: 16 sites carry caller
or service identity, 3 are tracked with removal plans *and* the tracked set is
provably current, and the identity rule covers generated SDKs, raw HTTP clients,
and `set_api_key` substitution. The three tracked sites remain blocked on the
decisions recorded in §15/§16, so the goal stays active.

## 18. Final verification and open decisions

Re-ran the verification set on the current working trees (round 12):

| Check | Result |
|---|---|
| `check-http-registry-resolution.mjs --workspace <root>` | passed: 6,268 files, 19 call sites, **16 aligned, 3 tracked** (stale-baseline rule active) |
| `cargo test -p sdkwork-utils-rust --lib` | 199 passed (caller + service identity helpers) |
| `cargo test -p sdkwork-web-core --lib caller` | 4 passed (request-scoped caller carrier) |
| `cargo test -p sdkwork-agents-tool-cloudrouter --lib` | 68 passed (credential precedence, both transports) |
| `cargo test -p sdkwork-knowledgebase-agent-provider --lib` | 31 passed (caller-scoped cloudrouter client) |
| `cargo test -p sdkwork-deploy-web-port` | 6 passed (header pair + fail-closed) |
| `cargo test -p sdkwork-deploy-content-provider-port` | 19 passed (two legs, pair + fail-closed) |
| `cargo test -p sdkwork-content-course-service --lib` | 16 passed |

Earlier in the migration: generations 70 lib + registry integration, canvas
24/7/5, notes 22/56/9/2, course workspace 25/26, webserver gateway 471 (WSL),
webserver daemon 25 + 4.

### What is complete

- Shared helpers: `sdkwork_utils_rust::caller_identity::CallerCredentials`
  (user-scoped pair, `apply_typed`, wire order, fingerprint, redacted Debug) and
  `sdkwork_utils_rust::service_identity::{ServiceCredentials, ServiceTokenKind,
  missing_service_credentials}` (machine pair, fail-closed diagnostic).
- Framework carrier: request-scoped `CALLER_CREDENTIALS` task-local injected with
  `WebRequestContext`, deliberately not inherited across spawn/blocking
  boundaries; adapters take the pair from the context or from a per-call client.
- 16 migrated call sites: every user-scoped site carries the caller pair on the
  destination's wire order; every machine site declares `identity:service` and
  presents the service-token + `Access-Token` pair, failing closed with the
  settings that are missing. Per-call clients are keyed by
  `(base URL, credential fingerprint)` where pooling exists.
- Gate: covers generated SDKs and raw HTTP clients, rejects `set_api_key`
  without `identity:service`, and now fails on stale baseline entries.

### Open decisions (the three tracked sites)

1. **aiot device speech turn** — the edge authenticates a device bearer and
   derives tenant/organization from the device association; the Xiaozhi session
   retains no end-user dual token and the cloud-router audio endpoints carry no
   tenant field. Decide: bind a user credential to the device association at
   pairing (forward the pair), or declare a device-level service identity with
   explicit attribution. Then implement `SpeechTurnInput` identity fields plus
   the pair application.
2. **image provider** — the adapter does not compile against the regenerated
   `cloudrouter_open_sdk` (midjourney/nano-banana models and client accessors
   removed, OpenAI image request reshaped, Vidu request field dropped). Decide
   the vendor surface, reconcile the adapter, then apply the §7 `with_caller`
   shape.
3. **voice** — the route is not wired; migrate with the same `with_caller` shape
   when it is.

The goal is reported blocked on those three decisions; the migration itself has
no unexplained or silently exempted site left.

## 19. Final round — the three decided sites (image, aiot, voice)

With the owner's decisions recorded in the decision note
(`2026-11-18-registry-identity-open-decisions.md`), all three remaining sites are
migrated and the gate is clean.

### image (`95eb48f`)

The regenerated cloudrouter open-api no longer publishes midjourney or
nano-banana image operations. The adapter drops those vendor routes (routing
returns no route, `validate` reports unsupported capability), keeps OpenAI image
generation and Vidu reference-to-image, and reconciles the request builders with
the regenerated models. Identity: `with_caller` on the provider trait,
`ImageGenerationService::with_caller` binding, clients pooled by
`(base URL, credential fingerprint)`, caller pair on a user dispatch, declared
open-api key only on the caller-less background/retrieval path, host binds the
caller from the app-api handler. Tests: pool key separation, OpenAI/Vidu request
contracts, no-route decisions, test doubles implement `with_caller`.

### aiot (`87c5fc8`)

`SpeechTurnInput` carries tenant/organization/user/device attribution, populated
from the device association already registered at the WebSocket handshake (new
`association_for_ws_session_id` lookup). The bridge declares `identity:service`
with its justification: the edge holds no end-user tokens and the cloud-router
ASR/TTS endpoints take no tenant field, so attribution travels with the turn
while the dispatch presents the declared machine identity. Tests: session lookup
returns the attribution; simulator turn construction uses defaults.

### voice (`b511d53`)

The speech request reconciles with the regenerated SDK (removed `metadata`
parameter now rejected explicitly), and the module gains
`build_voice_generation_service_for_caller` for a user-scoped route while the
caller-less builder stays the declared bootstrap path (marker with
justification). Tests: regenerated model mapping, metadata rejection.

### Final matrix

`node sdkwork-specs/tools/check-http-registry-resolution.mjs --workspace <root>`
→ 6,268 Rust files, **19 call sites, 19 aligned, 0 tracked**; the gate also
covers raw HTTP clients, rejects `set_api_key` without `identity:service`, and
fails on stale baseline entries.

All three sites carry per-site tests and build in their own workspaces (image
`cargo test --workspace`; aiot bridge 3 + edge runtime 50 in WSL with the system
libopus; voice adapter 4 + service 1 plus workspace check).

## 20. Post-completion gate sweep

After the migration closed, the cross-repository gates were run over every
repository this work touched:

- `check-rust-crate-naming-standard.mjs` and `check-rust-manifest-standard.mjs`
  over all 13 repositories (utils, web-framework, generations, canvas, notes,
  course, knowledgebase, agents, image, aiot, voice, deployments, webserver):
  **0 errors** everywhere.
- `check-rust-backend-composition.mjs` for image (its SPI/service gained the
  utils dependency): passed.
- `cargo metadata --locked` for image, voice and utils: lockfiles consistent.
- `check-embedded-self-loop.mjs` surfaced one **pre-existing** finding, not an
  identity defect: the nginx importer parsed `listen` ports inline instead of
  using the shared listener-bind helper. `sdkwork-webserver` `806d7360` delegates
  both the http and mail listen port parsing to
  `sdkwork_utils_rust::service_base_url::bind_port`; nginx tests pass in WSL
  (193) and the self-loop audit now passes.
- `check-http-registry-resolution.mjs`: 19 call sites, **19 aligned, 0 tracked**.

The remaining worktree entries are another session's edits
(`sdkwork-voice/Cargo.lock`, `sdkwork-specs/workspace/consumers/sdkwork-cloudrouter.json`)
and were left untouched.

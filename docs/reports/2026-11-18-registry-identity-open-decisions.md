# Registry Identity Propagation — Open Decisions And Recommendation

Date: 2026-11-18
Context: `docs/reports/2026-11-18-caller-identity-propagation-alignment.md` (§1–§18)

The caller-identity migration is complete for 16 of 19 registry-resolved call
sites. The three remaining sites are blocked on decisions that are not code
questions. This note records the evidence, the recommended decision for each, and
the exact change each decision unlocks.

## Decision 1 — aiot device speech turn

**Question:** should a device-originated speech turn authenticate as its owner
(caller pair) or as a declared device machine identity?

**Evidence**

- The Xiaozhi edge authenticates a **device bearer** (`xiaozhi_device_token_valid`
  → `resolve_association_for_bearer_token`) and derives scope from the device
  credential repository; the session retains no user dual token.
- The association record already carries the owner:
  `AiotStorageAssociation { tenant_id, organization_id, user_id: Option<i64>,
  owner_type, owner_id, data_scope }` (`sdkwork-aiot-storage`).
- The only cloud-router calls in this path are ASR (`/v1/audio/transcriptions`)
  and TTS (`/v1/audio/speech`) plus the kernel chat turn; none of them accepts a
  tenant/organization field, and there is no user request at the edge.

**Recommendation: B+, device-attributed machine identity.**

1. Add `tenant_id`, `organization_id`, `user_id`, `device_id` and an optional
   `caller` to `SpeechTurnInput`; populate the ids from the association at the
   edge.
2. Declare `identity:service` on the cloud-router client module: the turn is
   machine work authenticated by the device bearer, there is no session token to
   forward, and storing an owner refresh/access token on an edge device would
   enlarge the credential-theft surface for no protocol benefit.
3. Attribute usage to the association's tenant/organization/user in our own
   metering and logs; do not claim per-tenant identity at cloud-router.
4. Keep `caller` in the struct so the day a pairing flow can hand the edge a
   short-lived delegated token, the bridge can apply it without a second
   refactor.

**Why not A (bind the owner's credentials at pairing):** it requires the edge or
the association database to hold a long-lived user credential, which contradicts
the "no credentials on the edge" posture; if per-tenant billing/quto at
cloud-router is a hard requirement, the right variant is an IAM-issued *device
delegation token* (short-lived, scoped, revocable), which is a separate feature.

**Acceptance:** turn input carries the attribution; bridge declares
`identity:service`; a test proves a turn built from an association carries
tenant/org/user; gate entry removed.

## Decision 2 — image provider vendor surface

**Question:** which image vendors survive the `cloudrouter_open_sdk`
regeneration?

**Evidence**

- The current cloud-router open-api publishes **no midjourney or nano-banana
  paths at all** (`cloudrouter-open-sdk.openapi.json`: images exist under
  `/v1/images/generations`, `/v1/images/edits`, `/v1/images/variations`, plus
  `/vidu/ent/v2/reference2image` and other vendors such as bytedance, kling,
  minimax, stability, google, xai, zhipu, alibaba).
- The image repository's committed adapter does not compile against that SDK:
  removed `Midjourney*/NanoBanana*` models and client accessors, new
  `OpenAiImageGenerationRequest` fields, and `ViduReferenceToImageRequest.style`
  removed.

**Recommendation: drop midjourney and nano-banana image generation; keep OpenAI
image generation and Vidu reference-to-image.**

1. Remove those two dispatch routes, their request builders, SDK operation
   mappings, normalizers and tests from
   `sdkwork-image-generation-provider-adapter`; update the provider descriptor
   vendor list to `openai`, `gpt-image`, `vidu`.
2. Adapt `requests.rs` to the regenerated models (pass `None`/defaults for the
   new optional fields; drop `style`).
3. Then apply the caller-pair migration exactly as generations/canvas do:
   `with_caller` on the provider, clients keyed by
   `(base URL, credential fingerprint)`, the process key only on the
   background/retrieval path.
4. If midjourney/nano-banana matter to the product, re-add them when
   cloud-router restores the operations; the adapter should not carry dead
   vendor code it cannot compile.

**Acceptance:** `cargo test -p sdkwork-image-generation-provider-adapter` and the
image workspace build; a caller-bound dispatch carries both headers and fails
closed without a caller; gate entry removed.

## Decision 3 — voice

**Question:** migrate now or when the route is wired?

**Evidence:** the voice bootstrap module has no caller source (no wired route);
its client is built once from a process API key (zero callers found in the
audit).

**Recommendation: defer, keep the gate entry.** Migrating an unreachable module
would mean adding a caller parameter no one passes — a false "aligned" signal.
When the route is wired, apply the same `with_caller` shape and remove the entry
in that change.

## Execution order on approval

| Order | Work | Clears |
|---|---|---|
| 1 | image: drop two vendors, reconcile the adapter, thread the caller pair | gate 17 aligned / 2 tracked |
| 2 | aiot: attribution fields + declared identity + tests | gate 18 aligned / 1 tracked |
| 3 | voice: wire the route (separate feature), then migrate | gate 19 aligned / 0 tracked |

Steps 1 and 2 are independent and both follow patterns already proven in this
migration; step 3 is feature work, not identity work.

## What is needed from the owner

- Approve **B+** for aiot (device-attributed machine identity), or state that
  per-tenant cloud-router billing is mandatory (then the delegation-token feature
  is required first).
- Approve dropping **midjourney / nano-banana image generation** from the image
  provider (or name the vendor surface to keep).
- Confirm voice is deferred until its route is wired.

Everything already migrated stays as committed and verified
(`check-http-registry-resolution.mjs`: 19 call sites, 16 aligned, 3 tracked).

# 2026-11-09 — Model-id separators, and the 50301 that was actually an empty account pool

## 1. Symptom

`POST /app/v3/api/ai/agents/agent.chat.default/sessions/{sessionId}/turns`
returned 503 `50301` with

```json
{
  "detail": "cloud router turn failed: provider_error: cloud router chat completion failed: http status 503: {\"error\":{\"message\":\"upstream route snapshot is empty for model: deepseek/deepseek-flash (routing account-pool gates [enabled_suppliers=33, enabled_accounts=0, auth_method_matches=0, active_credentials=0, group_members=33, base_url_resolvable=0] block at 'enabled_accounts'; create or enable an upstream account under an enabled supplier)\",\"type\":\"server_error\",\"code\":\"upstream_route_snapshot_empty\"}}; 上游账号凭证被拒绝（401）：请在 Cloud Router 后台检查 DeepSeek 账号的 API Key 是否有效",
  "traceId": "9f428f98-308c-46c0-a90b-25d3ba749fea"
}
```

## 2. What the error actually says

The Cloud Router message is a **gate chain**, evaluated cumulatively against the
routing snapshot (`infrastructure/sql/queries/snapshot.rs`):

| gate | count | meaning |
| --- | --- | --- |
| `enabled_suppliers` | 33 | suppliers are fine |
| **`enabled_accounts`** | **0** | **no enabled upstream account under any supplier** |
| `auth_method_matches` | 0 | (never reached) |
| `active_credentials` | 0 | (never reached) |
| `group_members` | 33 | accounts are in groups |
| `base_url_resolvable` | 0 | (never reached) |

The gate counts `ai_upstream_account.status = 1` under an enabled supplier. Live
data on `api.noaper.com` (DB `sdkwork_ai_prod`):

```text
select status, count(*) from ai_upstream_account;              -- 0 | 33
select status, count(*) from ai_upstream_account_credential;    -- 0 | 33
```

All 33 seeded upstream accounts **and** all 33 credentials are disabled. That is
by design: `infrastructure/sql/ai_routing_seed.rs` inserts every account with
`DISABLED_STATUS` (`ai_routing_seed.rs:2489-2528`) and a **placeholder** secret
(`ai_routing_seed.rs:4538-4570`), because a real vendor key can only come from an
operator. So:

- **any** model id fails identically (`deepseek-flash`, `deepseek/deepseek-flash`,
  `openai/gpt-4o`, ...) — the failure happens before model-specific routing;
- the trailing "上游账号凭证被拒绝（401）" hint was wrong: agents mapped any 5xx
  body containing `upstream`/`credential` to the 401 hint
  (`sdkwork-agents-tool-cloudrouter/src/client.rs`), which sent the operator to
  the API key instead of the disabled account.

**Remediation (operator, Cloud Router 后台):** 上游账号 → DeepSeek →
启用账号并写入有效的 API Key 凭证（凭证状态随之启用）；账号组已包含该账号
（`group_members >= 1`），Base URL 走供应商端点默认值即可。启用后同一请求即可
进入模型路由，与模型 id 的写法无关。

## 3. Model-id separator contract (requested work)

Independent of the pool state, the inbound model id was only tolerant of `/`
and `:`; `|` and a `region` middle segment did not resolve, and the outbound
provider model derivation understood only `/`. Both layers now share one
contract.

### 3.1 Separator matrix

| inbound id | catalog lookup | provider model sent upstream |
| --- | --- | --- |
| `deepseek/deepseek-flash` | exact key | `deepseek-flash` |
| `deepseek:deepseek-flash` | `:` rewritten to `/` | `deepseek-flash` |
| `deepseek|deepseek-flash` | **new**: `|` rewritten to `/` | **new**: `deepseek-flash` |
| `deepseek：deepseek-flash` (full width) | yes | **new**: `deepseek-flash` |
| `deepseek｜deepseek-flash` (full width) | **new** | **new** |
| `deepseek／deepseek-flash` (full width) | **new** | **new** |
| `deepseek/cn/deepseek-flash` | part-slug probe | **new**: region dropped |
| `openrouter|meta-llama|llama-3-70b` | relay namespace stripped | `meta-llama/llama-3-70b` |
| `gpt-4o:free` | variant suffix preserved | `gpt-4o:free` (unchanged) |
| `ft:gpt-4o-2024-08-13:org:abc` | head is not a vendor | unchanged (never mangled) |

### 3.2 Implementation

- `domain/model_request_alias.rs`
  - `normalize_wire_model_id` folds `：`/`｜`/`／` and rewrites `:` **and `|`**
    separators to `/` (variant suffixes split off first).
  - new `provider_native_model_id_for_wire`: normalize → drop one `region`
    middle segment → strip a known vendor/relay prefix. Spellings that strip no
    catalog prefix are returned verbatim, so fine-tune and variant ids survive.
- `application/invocation/route_planning.rs` and `api/openai_runtime.rs`:
  both `normalized_resolved_provider_model` copies now derive the provider model
  through that helper (they had duplicated `provider_native_model_id`).
- `api/app_runtime.rs`: the app-runtime surface resolves `vendor:model` /
  `vendor|model` dialects to the native id; a canonical `vendor/model` key stays
  verbatim there because an OpenRouter-style relay account legitimately dials
  with the qualified id.

### 3.3 Hint fix

`sdkwork-agents-tool-cloudrouter/src/client.rs` now maps
`upstream_route_snapshot_empty` / `routing account-pool gates` to the reported
gate (`block at '<gate>'`) with that gate's own remediation, before the generic
credential branch.

## 4. One parser for every provider-facing model (media included)

The dialect rules lived in the router. They now live in the model catalog —
the owner of the model vocabulary — behind a single public function:

- `sdkwork-models-catalog-service::domain::provider_native_model_id` — the one
  parser every surface calls. It accepts `/`, `:`, `|`, their full-width forms,
  and one `region` middle segment; it strips a vendor/relay prefix only when the
  head is a known vendor or relay namespace, so `ft:gpt-4o-...:org:abc` and
  `gpt-4o:free` are returned verbatim.
- `parse_request_model_identity` — the tolerant reader behind it (vendor + model
  parts). The **storage** contract stays strict: `ensure_canonical_model_catalog_key`
  still demands `vendor/model` for catalog rows.
- The router's private duplicate (`provider_native_model_id_for_wire`,
  `without_region_segment`) is deleted; `route_planning`, the openai runtime, the
  app-runtime, and every other caller use the catalog function.

Every outbound surface now passes through **one chokepoint**
(`application/invocation/provider_request.rs::normalized_provider_model`): the
model written into the JSON/multipart body, the query, and the
`models/{model}:{action}` path is the catalog-native id. That covers chat,
responses, embeddings, **image, video, audio, music**, and every vendor-native
passthrough route (Anthropic, Gemini, Kling, Vidu, Volcengine, ...), because all
of them are dialled through that one builder.

Two defects surfaced while wiring the media paths:

1. `rewrite_models_action_path` recognized only `/v1beta/models/` and
   `/v1/models/`. Google's media surfaces publish
   `/google/v1beta/models/{model}:generateImages` (and `:generateVideos`), so the
   **main image/video path forwarded the caller's model verbatim**. The
   `/google/v1beta/models/` prefix is now recognized.
2. The action split took the first `:`, so a dialect model in the path
   (`models/google:veo-3.0:generateVideos`) produced a mangled action. It now
   splits at the last `:` (the action), replacing the whole model.

## 5. Verification

```text
# sdkwork-models (unified parser)
cargo test -p sdkwork-models-catalog-service --lib
  test result: ok. 86 passed; 0 failed

# cloudrouter (routing, derivation, outbound chokepoint)
cargo test -p sdkwork-cloudrouter-router-service --lib
  test result: ok. 686 passed; 0 failed

# agents (gate hint)
cargo test -p sdkwork-agents-tool-cloudrouter --lib -- --test-threads=1
  test result: ok. 67 passed; 0 failed
```

New tests:

- `domain::catalog::tests::provider_native_model_id_strips_every_request_separator`
- `domain::catalog::tests::provider_native_model_id_drops_the_region_segment`
- `domain::catalog::tests::provider_native_model_id_keeps_model_side_segments_and_relay_overlays`
- `domain::catalog::tests::provider_native_model_id_leaves_non_vendor_heads_untouched`
- `domain::catalog::tests::request_identity_parsing_is_separate_from_storage_validation`
- `domain::model_request_alias::tests::normalization_rewrites_pipe_separators_both_widths`
- `application::invocation::route_planning::tests::provider_model_derivation_strips_every_separator_spelling`
- `application::invocation::route_planning::tests::provider_model_derivation_keeps_deliberate_mappings`
- `application::invocation::provider_request::tests::provider_model_normalization_covers_every_separator_spelling`
- `application::invocation::provider_request::tests::media_action_paths_are_rewritten_to_the_native_model`
- `api::app_runtime::tests::runtime_provider_model_resolves_dialect_separators`
- `client::tests::empty_account_pool_reports_the_blocking_gate_not_a_401`

Note: `chat_stream::tests::both_transports_present_the_same_credential_source`
fails only under parallel execution (it mutates process env) and passes with
`--test-threads=1`; that flake predates this change.

## 6. Follow-ups

- Operator: enable one upstream account with a real vendor key (section 2). No
  code change makes a disabled/placeholder account routable.
- Consider a startup warning that logs the gate chain once when the routing
  snapshot is empty, so an all-disabled pool is visible before the first turn.
- Media adapters that dial a vendor with a *constant* model (for example the
  ElevenLabs sound-effect default) are intentionally untouched: the constant is
  already native and no request id can reach it.

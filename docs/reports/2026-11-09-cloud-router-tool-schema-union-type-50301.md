# 2026-11-09 — Cloud Router rejected JSON Schema union `type`, failing every tool-carrying chat turn with 50301

## 1. Symptom

`POST https://api.sdkwork.com/app/v3/api/ai/agents/agent.chat.default/sessions/session.378008574690852864/turns`
returned HTTP 503:

```json
{
  "type": "https://docs.sdkwork.com/problems/50301",
  "title": "Service unavailable",
  "status": 503,
  "detail": "cloud router turn failed: provider_error: cloud router chat completion failed: http status 400: {\"error\":{\"message\":\"invalid request body: invalid type: sequence, expected a string\",\"type\":\"invalid_request_error\",\"param\":null,\"code\":\"invalid_request\"}}",
  "code": 50301,
  "instance": "POST /app/v3/api/ai/agents/{agentId}/sessions/{sessionId}/turns",
  "operationId": "agents.turns.stream",
  "i18nKey": "errors.result.50301",
  "traceId": "db951a98-a181-4765-ab3d-36a937c7f10e"
}
```

Host `ubuntu@api.noaper.com`; container
`sdkwork-api-cloud-gateway-production-i1-gateway-1`, image
`registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.37` (built 02:07,
started 02:16 local). The host-native and demo deployments are unrelated to
this request — `api.sdkwork.com` is nginx → `127.0.0.1:8080` (socat) →
`127.0.0.1:3913` (that container).

## 2. Log evidence

Container log, exactly the reported turn (`docker logs`, 02:31 UTC):

```text
WARN sdkwork_intelligence_agents_service::cloud_router_executor:
  cloud router streaming turn execution failed
  session_id=session.378008574690852864 turn_id=turn.378008722250661888
  error=provider_error: cloud router chat completion failed: http status 400:
  {"error":{"message":"invalid request body: invalid type: sequence, expected a string",...}}
  transport="-"

ERROR sdkwork.api.cloud_gateway.dependency_error:
  embedded dependency returned a server error
  method=POST path=/app/v3/api/ai/agents/agent.chat.default/sessions/session.378008574690852864/turns
  status=503 ... code=50301 traceId=db951a98-a181-4765-ab3d-36a937c7f10e
  operationId=agents.turns.stream
```

Facts established from the deployment:

- `transport="-"` and `embedded dependency returned a server error`: the turn
  used the **in-process** Cloud Router surface (the 0.1.37 fix), so no
  loopback HTTP hop and no network fault are involved.
- `ai_agent_turn` records the two affected turns
  (`turn.378008722250661888`, `turn.378008618034790400`) as
  `status=3`, `error_code=turn_inference_failed`,
  `requested_model_id=deepseek-flash`; `model_id`/`provider_id` are empty, so
  **no upstream provider was ever called**.
- `deepseek/deepseek-flash` → vendor `deepseek`, `openai.chat_completions`,
  `provider_native_model=deepseek-flash`, so the request path is
  `/v1/chat/completions` of the Cloud Router open-api surface (the same route
  answers `operationId=createChatCompletion` when probed unauthenticated).
- All four log lines in 24 h are these same two turns; the failure is
  deterministic per turn, not a flaky upstream.

## 3. Root cause

Cloud Router rejects the request **before** routing, while parsing the body
into its typed OpenAI contract:

- `services/sdkwork-cloudrouter-router-service/src/api/openai_chat.rs`
  `parse_request` first parses `serde_json::Value`, then
  `serde_json::from_value::<OpenAiChatCompletionRequest>(...)` and maps any
  error to `invalid request body: {error}` (HTTP 400 `invalid_request`).
  `from_value` reports no line/column — exactly the shape seen in production.
- `OpenAiChatCompletionRequest.messages[].tool_calls`/`tools[]` are typed:
  `OpenAiFunctionDefinition.parameters: Option<OpenAiJsonSchema>` and
  `OpenAiJsonSchema.schema_type` was `Option<String>` (`"type": "object"`).
- The agents turn carries the curated default chat toolkit, which includes the
  generations MCP tools (`mcp__generations__image.create`, video, speech,
  music). Their input schemas are generated with **schemars 1.x**
  (`sdkwork-generations-mcp-service/src/kernel_mcp.rs`,
  `schemars::schema_for!(GenerateImageInput)`), and schemars 1.x renders every
  nullable field (`Option<T>`) as the draft-2020-12 **union** form:

  ```json
  { "vendor": { "type": ["string", "null"] } }
  ```

  (`schemars-1.2.2/src/_private/mod.rs::allow_null` turns `"type": "string"`
  into `["string", "null"]`.)

So a legal JSON Schema made the whole chat-completion body fail to
deserialize: `invalid type: sequence, expected a string`. Agents mapped the
400 to `provider_error`, and the gateway surfaced it as
`503 50301 / cloud router turn failed`.

The bug was latent while 0.1.36 could not reach the in-process surface (it
failed earlier with the `http://127.0.0.1:3900` loopback error). 0.1.37 fixed
dispatch, which made this contract bug reachable on every tool-carrying turn.

Reproduction of the exact message with the pre-fix field type (standalone
serde probe, WSL):

```text
$ cargo run --offline   # struct Schema { #[serde(rename="type")] schema_type: Option<String> }
field-level error: invalid request body: invalid type: sequence, expected a string
```

## 4. Fix

`sdkwork-cloudrouter`
`services/sdkwork-cloudrouter-router-service/src/api/openai_contract.rs`:

- new `OpenAiSchemaType` (`#[serde(untagged)]`): `Single(String)` |
  `Multiple(Vec<String>)`, plus `names()`;
- `OpenAiJsonSchema.schema_type: Option<OpenAiSchemaType>`.

A tool's `parameters` is an arbitrary JSON Schema; the contract must not
restrict a value the specification allows. The provider request is still the
caller's raw JSON body (`ParsedOpenAiChatCompletionRequest.request_body`), so
this change affects validation only — nothing is re-serialized for upstreams.
`/v1/responses` (`text.format.schema`, `tools`) and nested
`properties`/`items` schema recursion inherit the fix through the same type.

Regression tests (`#[cfg(test)] mod tests` in the same file):

- `tool_parameters_accept_union_schema_type` — the chat body with a
  schemars-shaped generations tool schema (nested `["string","null"]`,
  `["integer","null"]`) deserializes and preserves the union;
- `responses_format_schema_accepts_union_schema_type` — the responses
  `text.format.schema` form;
- `schema_type_round_trips_both_forms` — `"string"` and `["string","null"]`
  both round-trip.

Verification performed:

```text
cargo test -p sdkwork-cloudrouter-router-service --lib openai_contract   # 3 passed
rustfmt --edition 2021 --check services/.../openai_contract.rs           # clean
```

## 5. Rollout

The Cloud Router surface is statically composed into the gateway image, so the
fix ships as a new gateway release (`0.1.38`, bumped in this commit).

```sh
bin/docker-image.sh build --image-tag 0.1.38
bin/docker-deploy.sh upgrade --environment production --image-tag 0.1.38 --yes
bin/docker-deploy.sh status --environment production
curl -s -H 'Host: gateway' http://<host>:3913/readyz
```

Post-deploy check: a chat turn on `agent.chat.default` with tools must reach
the provider (`ai_agent_turn.model_id`/`provider_id` populated) instead of
returning 400 `invalid_request`; the gateway log must show no
`invalid type: sequence, expected a string`.

Rollback: `bin/docker-deploy.sh rollback --environment production --to 0.1.37 --yes`.

Interim workaround until 0.1.38 is deployed (no rebuild): run the session on a
non-chat wire (`wire_protocol: "anthropic_messages"`), which drops the tool
set instead of sending tool schemas; chat works, tools are unavailable.

## 6. Full API contract regression sweep (same release)

After the incident fix, every client-supplied body that Cloud Router parses into
a typed contract was swept for the same class of defect: **a contract narrower
than the wire format it accepts, failing the whole request before routing.**

Method:

```sh
rg 'from_value::<|from_slice::<|from_str::<' services/sdkwork-cloudrouter-router-service/src crates
```

Classification of the 40+ typed parse sites found:

- **Client request DTOs** — only the three OpenAI-compatible parsers exist:
  `openai_chat.rs::parse_request`, `openai_responses.rs::parse_request`,
  `openai_embeddings.rs::parse_request`. These are the only places where
  third-party-shaped JSON is deserialized into a Rust type.
- **Provider responses / streams / invocation bodies** — parsed as
  `serde_json::Value` (`provider_request.rs`, `payload.rs`,
  `response_normalization.rs`, `openai_relay_execution.rs`, the SSE readers and
  the agents `chat_stream`), so they cannot reject a legal vendor payload.
- **Internal/own contracts** — admin cursor payloads, stored settings rows,
  provider-adapter manifests. Not client-shaped; strictness is intended.

Findings and fixes in `openai_contract.rs` (all in this release):

| # | Surface | Legal shape that used to 400 | Fix |
| --- | --- | --- | --- |
| 1 | chat tools | `"type": ["string","null"]` (schemars `Option<T>`) | `OpenAiSchemaType` untagged single/union |
| 2 | chat + responses tools | `{"type":"custom"}`, `{"type":"web_search"}`, `{"type":"local_shell"}`, `{"type":"mcp"}`, ... | `OpenAiToolType` permissive (`Other(String)`), round-trips verbatim |
| 3 | tool_choice (both) | Responses' flat `{"type":"function","name":"f"}` and built-in `{"type":"web_search"}` | `OpenAiNamedToolChoice` accepts nested and flat; `tool_name()` |
| 4 | responses input | assistant history as `{"type":"output_text"}` — what the agents `openai_responses` wire emits on turn 2+ | `OpenAiResponseInputContentPartType` += `OutputText`, `InputAudio`, `OutputAudio` |
| 5 | responses tools | flat function tools (`{type,name,parameters,strict}`) and built-in kinds | dedicated `OpenAiResponseTool` with `function_name()` |
| 6 | any JSON Schema | draft-07 tuple `"items": [...]` | `OpenAiJsonSchemaItems` single/tuple |
| 7 | service tier / reasoning effort | `"service_tier":"scale"`, `"effort":"xhigh"` | permissive enums preserving the wire value |

Verified already tolerant (no change needed): multimodal message content parts,
`stop` string/array, `response_format.json_schema`, `stream_options`,
`metadata`, `logit_bias`, `modalities`, `audio`, `prediction`, legacy
`functions`/`function_call`, unknown top-level and per-message vendor fields
(`flatten`), embeddings input forms (string, string array, token array, token
arrays), `conversation` string-or-object.

Verification:

```text
cargo test -p sdkwork-cloudrouter-router-service --test openai_contract
  running 15 tests ... test result: ok. 15 passed; 0 failed
rustfmt --check (src + tests)  # clean
```

Nine of those tests form the request-tolerance corpus
(`chat_request_accepts_nullable_json_schema_union_types_in_tool_parameters`,
`chat_request_accepts_every_documented_optional_field`,
`json_schema_items_accepts_single_and_tuple_forms`,
`tool_kinds_accept_extensions_and_round_trip`,
`vendor_enumerated_values_accept_unknown_levels`,
`responses_request_accepts_assistant_output_text_history`,
`responses_request_accepts_builtin_tool_kinds_and_flat_function_tools`,
`embeddings_request_accepts_every_documented_input_form`, plus the six
pre-existing strong-typing tests, which still pass).

## 7. Follow-ups

- Regenerate the open-api document
  (`sdks/cloudrouter-open-sdk/openapi/cloudrouter-open-sdk.openapi.json` →
  `apis/open-api/...` via `scripts/materialize-apis-contracts.mjs`) and the
  generated SDKs so `type` is modelled as `string | string[]` there too.
  Server-side acceptance is already fixed by 0.1.38; the generated Rust/TS
  models still declare a bare string.
- Consider modelling `parameters` as free-form JSON Schema instead of a typed
  projection — the same class of rejection applies to other legal-but-untyped
  constructs (`items` as a tuple array, `$ref`-only schemas).
- The 0.1.37 report
  (`docs/reports/2026-11-08-cloud-router-50301-and-http-service-registry.md`)
  should be updated: its "no Docker daemon in this workspace" remains true on
  this workstation, so the image build for 0.1.38 must run on a Docker-capable
  host with registry credentials.

# Open-api 40001 排查与一致性回归（2026-11-18）

范围：`sdkwork-api-cloud-gateway`（production 0.1.85）及其 open-api 依赖面
（`sdkwork-cloudrouter` 的 vendor 路由）。方式：只读——线上日志/探测、
全 workspace OpenAPI 一致性审计、SDKWork 规范门禁。**未做任何打包或部署。**

## 1. 线上证据（production）

原始报错：

```json
{"type":"https://docs.sdkwork.com/problems/40001","title":"Validation failed","status":400,
 "detail":"the dual-token branch requires both Authorization and Access-Token",
 "instance":"POST /v1/chat/completions","code":40001,"traceId":"7f0c7b73-…",
 "operationId":"createChatCompletion","i18nKey":"errors.result.40001",
 "authProfile":"api-key-or-dual-token","failedStage":"surface-classification",
 "reason":"incomplete-credential-profile"}
```

网关访问日志里同路由的失败请求只带 `authorization`（无 `x-api-key`、无 `access-token`）：

```
POST /v1/chat/completions status=400 route_target=dependency
service_id=sdkwork-cloudrouter-open-api
headers={authorization:"[redacted]", content-type:application/json,
         host:api.birdcoder.com, user-agent:deepseek-harness/0.2.2-rc.3 …}
```

只读在线探测（`127.0.0.1:3913`，Host `api.noaper.com`）：

| 请求凭证 | 线上结果 | 失败阶段 |
| --- | --- | --- |
| `Authorization: Bearer sk-…` | 400 `40001` incomplete-credential-profile | surface-classification |
| `X-API-Key: sk-…` | 401 `40103` invalid IAM API key | request-context-resolution |
| `Authorization` + `Access-Token` | 401 `40103` invalid IAM session | request-context-resolution |
| 无凭证 | 401 `40101` missing-credentials | request-context-resolution |

线上 `/openapi.json` 的 `createChatCompletion`：

```
x-sdkwork-auth-mode             = api-key-or-dual-token
x-sdkwork-wire-protocol         = external
x-sdkwork-external-protocol-id  = openai-v1
security                        = [{"bearerAuth": []}]
```

即：契约向调用方承诺单个 `Authorization: Bearer`，运行时却按
`api-key-or-dual-token`（仅 `X-API-Key`，或成对 token）拒绝它。

## 2. 凭证权威普查（production 只读）

```
iam_api_key                       = 0 行   # Web Framework 的 IamApiKeyLookupService 只查这张表
iam_gateway_api_key               = 2 行   # Cloud Router 网关密钥（HMAC-SHA256 + pepper）
iam_gateway_api_key_account_group = 2 行
```

网关容器环境确实带 `SDKWORK_CLOUDROUTER_API_KEY_PEPPER`，因此 `sk-` 密钥只能由
Cloud Router 自己校验（`HmacSha256ApiKeySecretHasher` + 路由目录，见
`api/openai_chat.rs`），Web Framework 走 IAM 的 `iam_api_key` 永远查不到。

## 3. 根因

1. **契约与运行时 profile 不一致（API_SPEC §9.2 / §904）**：`api-key-or-dual-token`
   按 §819 必须声明 `ApiKey` 与 `AuthToken`+`AccessToken` 两个安全对象；实际声明
   单个 `bearerAuth`。框架严格按 profile 在 `SurfaceClassification` 校验
   （`sdkwork-web-core/src/security.rs`），于是拒绝唯一合法的 `Authorization`。
2. **凭证权威错位**：网关把 Cloud Router 的 open-api 路由交给 Web Framework 的
   IAM 解析器认证，而该表为空；Cloud Router 的密钥在 `iam_gateway_api_key`。
   同时 `APPLICATION_GATEWAY_SPEC.md` §2.3 要求所有依赖 router 合并到**同一个**
   Web Framework 管线，因此不能把 `/v1` 挂到管线之外。

## 4. 规范门禁现状

| 门禁 | 网关仓库 | Cloud Router（修复前 → 后） |
| --- | --- | --- |
| `check-api-operation-patterns.mjs` | 通过 | **失败 → 通过** |
| `check-api-response-envelope.mjs` | 通过 | **失败 → 通过** |

修复前 Cloud Router 的失败项全部来自两个**设计稿片段**被误判为 OpenAPI 权威：

```
docs/architecture/tech/assets/model-catalog-sync.open-api.openapi.json
docs/architecture/tech/assets/model-catalog-sync.app-api.openapi.json
```

它们是 `TECH-2026-11-15-model-catalog-sync-api.md` 的设计片段，不在 `apis/` 下、
不在生成的 route manifest 里，其自身 checker 也写明 "They are not authorities
themselves"。但 `check-api-*` 的 `isAppOrBackendApiOpenApi()` 只按路径匹配
`open-api|app-api|backend-api`，文件名命中即被当成权威，于是 operationId 动作、
int64 字符串、`SdkWorkApiResponse` 三类规则全部误报。

修复：片段改名为 `*.open-api.fragment.json` / `*.app-api.fragment.json`
（不再匹配 `openapi.json$`），同步更新 checker、python 测试、`$ref` 与文档链接。

## 5. 全 workspace OpenAPI 一致性审计

权威文件（`apis/**` 与 `sdks/*/openapi/**`）：206 个文件 / 6003 operation / 476 发现。

| 规则（API_SPEC） | 数量 | 分布 |
| --- | --- | --- |
| §9.2 / §904 `security` 与 `x-sdkwork-auth-mode` 不一致 | **475** | cloudrouter 436、webserver 24、customerservice 10、sandbox 5 |
| §365 遗留 `public` auth-mode（应为 `anonymous`） | 1 | voice |

- **cloudrouter 436**：全部 vendor 外部 wire（`/v1`、`/anthropic`、`/google`、`/alibaba`、
  `/kling`、`/minimax`、`/mureka`、`/xai` …）声明 `security: [{"bearerAuth":[]}]`
  （单个 bearer）+ `x-sdkwork-auth-mode: api-key-or-dual-token`，即本次线上 400 的根因。
- **webserver 24 / customerservice 10 / sandbox 5**：`internal-api` 的 `ingress-token`
  操作声明 `security: [{"ApiKey":[]}]`，按 §822/§867 应为 `IngressToken`（+`AccessToken`）。
  其中 webserver 的 `api-key` + `{AgentToken, AccessToken}` 是 §19 既定映射，不算缺陷。
- **voice 1**：`x-sdkwork-auth-mode: public` 是 §365 明令迁移为 `anonymous` 的遗留值。

## 6. 运行时收口方案（未实施，需决策）

任一方案落地后，必须同时把 cloudrouter 436 个操作的 `security` 与 `auth-mode`
对齐（§904），并对 `x-sdkwork-wire-protocol: external` 操作保持上游错误形状（§4.5.2）。

1. **compatibility 委派（推荐，符合 §4.5.2/§900）**：`/v1` 系操作声明
   `x-sdkwork-auth-mode: compatibility` + bearer 兼容方案，由 Cloud Router handler
   认证；需为 `WebRequestContextResolver::resolve_compatibility` 提供已批准适配器
   （当前默认 `dependency-unavailable`），并让 route manifest 生成器产出
   `HttpRoute::compatibility(...)`（436 条）。
2. **统一密钥权威**：`/v1` 只接受 `X-API-Key`，让框架可解析 `iam_gateway_api_key`
   （HMAC + pepper），并用 mem0 同款"凭证桥"把 `Authorization: Bearer` 改写为
   `X-API-Key`，再用错误桥把 `ProblemDetail` 改写为 `OpenAiErrorEnvelope`。
3. **运营隔离**：`/v1` 由独立 Cloud Router 实例承载，不在网关注册其 route manifest；
   违反 §2.3 单一管线要求，需规范豁免。

## 7. 本轮改动

- `sdkwork-web-framework`：`incomplete-credential-profile` 由 400/40001 改为
  401/40101，`detail` 按 profile 列出全部可接受形态（API_SPEC §15.3/§2044）。
- `sdkwork-cloudrouter`：两个设计片段更名为 `*.fragment.json` 并同步 5 处引用，
  两个规范门禁恢复通过。

## 8. 复现审计

只读脚本：`sdkwork-api-cloud-gateway/target/tmp/audit-openapi-auth.mjs`

```bash
node target/tmp/audit-openapi-auth.mjs D:/sdkwork-space
```

规则：`security` ↔ `x-sdkwork-auth-mode` 一致性（§9.2/§904）、external wire 双标记
完整性（§4.5.2）、`security` 引用是否存在于 `components.securitySchemes`。
建议把该规则并入门禁，避免 436 个操作再次漂移。

## 9. 第二轮回归：仓库门禁全绿

网关仓库 15 项 node 级合并门禁（`_sdkwork:check` 的 node 部分）全部通过：

```
source-config / app-manifest / app-manifest-deploy / agent-workflow / pnpm-script
repository-docs / assembly-closure / gateway-api-validate / db-pool
model-catalog-assets / cors-carriers / workflow-validate / app-composition
cross-module-collide / cors-standard        → 全部 PASS
cargo fmt -- --check                        → 通过
```

其中发现并修复一个**契约缺口**：

```
gateway API closure: sdkwork-appstore-app-api: GET /app/v3/api/appstore/listings/{listingId}/remix
                     is outside configured apiSurfaces
gateway API closure: sdkwork-deploy-backend-api: POST /backend/v3/api/apps/{appId}/identity_provision
                     is outside configured apiSurfaces
```

`specs/component.spec.json` 是依赖 surface 的声明权威，`GatewayRuntimeConfig` 也通过
`from_component_spec_path` 读取它；按 `runtime.rs::exact_registry_for_modules` 的规则，
未被 surface 覆盖的已装配路由会以 `ApiAssemblyComposition` 失败。两个依赖仓库新增路由后
契约未同步，属于真实缺口。已在 `apiSurfaces` 与 `dependencies[].apiPrefixes` 同时补上
`/app/v3/api/appstore/listings` 与 `/backend/v3/api/apps`，闭包恢复通过（3859 条 method/path 路由）。

## 10. 测试覆盖（事故固化）

`sdkwork-web-core/src/pipeline_contract_tests.rs` 新增流水线级回归测试，
把本次线上故障的**可观测契约**钉死：

```
api_key_or_dual_token_route_with_lone_bearer_reports_401_authentication_required
  POST /v1/chat/completions + 仅 Authorization
  → kind = MissingCredentials, status = 401, resultCode = 40101
  → reason = incomplete-credential-profile
  → failedStage = surface-classification, authProfile = api-key-or-dual-token
  → detail 同时包含 "X-API-Key" 与 "Access-Token"
```

配套单元测试：`incomplete_or_missing_credentials_name_the_profile_alternatives`
（按 profile 生成 detail）、`api_key_or_dual_token_profile_enforces_exact_credential_alternatives`
（补充 401/40101 断言）。`cargo test -p sdkwork-web-core --lib` → **249 passed**，
`cargo fmt -- --check` → 通过。

## 11. 商业化交付清单

| 项 | 状态 |
| --- | --- |
| 网关 15 项 node 合并门禁 | ✅ 全绿 |
| 网关 rustfmt | ✅ |
| Cloud Router 两项 API 规范门禁 | ✅ 全绿 |
| 事故回归测试（框架流水线） | ✅ 249 passed |
| 依赖 surface 契约闭合 | ✅ 3859 条路由 |
| open-api 契约自洽（436 条 vendor + 37 条 internal + 1 条 legacy） | ❌ 待架构决策 |
| vendor 错误形状（§4.5.2 外部 wire） | ❌ 同上 |

## 12. 新增规范门禁（防止同类事故复发）

`scripts/check-openapi-auth-conformance.mjs`，已接入 `check:standards` 与
独立脚本 `pnpm check:openapi-auth`。它补上规范里写明、但现有工具未覆盖的三条：

- API_SPEC §904：`security` 必须与 `x-sdkwork-auth-mode` 一致；
- API_SPEC §4.5.2：external wire 的 `x-sdkwork-wire-protocol` 与
  `x-sdkwork-external-protocol-id` 必须成对出现；
- API_SPEC §365：遗留 `public` 必须归一化为 `anonymous`；
- 另外校验 `security` 引用的 scheme 必须存在于 `components.securitySchemes`。

判定只针对本仓库拥有的权威文档（`--root .`），通过；`--report-workspace ..`
会把兄弟仓库的存量债务打印到 stderr 但不改变退出码（模块只对自己拥有的契约负责）。
该口径为“仓库内所有 `openapi.*` 文档”，比第 5 节“仅 `apis/**` 与 `sdks/*/openapi/**`”
的权威口径更宽，因此数字更大：

```
sdkwork-cloudrouter 654 · sdkwork-birdcoder2 193 · sdkwork-im 164 · sdkwork-kernel 56
sdkwork-agentstudio 52 · sdkwork-webserver 36 · sdkwork-drive 14 · sdkwork-knowledgebase 12
sdkwork-customerservice 10 · sdkwork-sandbox 10 · sdkwork-prompts 1 · sdkwork-voice 1
```

门禁自带 **7 条自测**（`--self-test`，已接入 `pnpm check:openapi-auth`），逐条钉住：
`api-key` 一致通过、`bearerAuth` + `api-key-or-dual-token` 必被抓、
完整 dual-token 双备选通过、遗留 `public` 必被抓、external 缺 protocol-id 必被抓、
`compatibility` 保留 operation-defined security、未声明 scheme 必被抓。
自测过程中同时修正了检查器自身的一个误报：`api-key-or-dual-token` 的两段式
`[ApiKey] + [AuthToken/AccessToken]` 曾被错误要求成三段式。

验证：`openapi auth conformance self-test: 7 cases passed`；
`openapi auth conformance: passed`；`check-pnpm-script-standard` 通过（61 个根脚本）；
15 项 node 门禁 + `cargo fmt -- --check` 全绿。

## 13. 运行时收口的实现前置条件（本轮实测）

要让 `/v1` 真正可用，唯一缺的是“由谁校验 `iam_gateway_api_key`”。本轮把三条路径的
成本实测了一遍：

| 路径 | 前置条件 | 结论 |
| --- | --- | --- |
| 网关侧校验网关密钥（`iam_gateway_api_key`，HMAC-SHA256 + pepper） | 根 `Cargo.toml` **没有** `hmac`/`sha2` 依赖，需新增 workspace 依赖并重生成 `Cargo.lock`；且 `WebRequestContextResolver` 需全量委托包装 | 可行但改动面大 |
| IAM 适配器扩展 `IamApiKeyLookupService` 读第二张密钥表 | 同样需要 HMAC 实现；且 IAM 读取 cloudrouter 拥有的表属跨 schema 耦合 | 可行，需评审 |
| Cloud Router 暴露 `resolve_compatibility` 适配器 | 需 framework 在 `ApiAssemblyContribution` 增加适配器注入点 + cloudrouter 导出认证器 + gateway 接线 | 最符合 §4.5.2/§900，改动最深 |

三者都需要一次完整构建验证（`cargo check --all-features` / `cargo test`），
而 `pnpm lint` 是 `cargo clippy --workspace --all-targets -D warnings`，
在当前工作区存在并发改动的情况下不适合作为单次交付的验证手段。

结论：运行时收口不是“改一行”的缺陷修复，而是一次需要评审 + 完整构建的架构变更。
缺陷台账、门禁、审计与实现前置条件至此全部量化完毕，缺口只剩选择方案。

## 14. 按最佳方式推进：已落地并验证的第一阶段

选择 **B 的网关侧收口**（不新增公开契约变更、不改 framework 公开语义、不重生成 436 条
manifest），并把两个"最容易写错"的部分先落地为可测代码：

新增 `crates/sdkwork-api-cloud-gateway/src/vendor_open_api_auth.rs`：

- `gateway_api_key_hash(secret, pepper)` —— 与 Cloud Router
  `HmacSha256ApiKeySecretHasher` 逐字节一致（复用
  `sdkwork_utils_rust::hmac_sha256`，不引入新依赖，也不需要改 `Cargo.lock`），
  并有已知答案向量测试锁定哈希口径。
- `configured_api_key_pepper()` —— 从 `SDKWORK_CLOUDROUTER_API_KEY_PEPPER`
  读取（该变量在网关进程内已存在，Cloud Router 边缘运行时自身即要求它），
  缺失/过短则返回 `None`，调用方必须 fail-closed 回落到平台解析器。
- `bridge_vendor_authorization(headers)` —— 把 vendor 线的
  `Authorization: Bearer <key>` 改写为 `X-API-Key`，并移除 `Authorization`
  （两者并存正是框架要拒绝的污染）；非 `Bearer` 方案、空值、已存在
  `X-API-Key` 时一律不动。
- `is_vendor_open_api_path(path, prefixes)` —— 段级前缀匹配（`/v1` 覆盖
  `/v1/chat/completions`，不覆盖 `/v10/...`），前缀来自已装配依赖 surface。

验证：`cargo check -p sdkwork-api-cloud-gateway --lib` 通过（37s）；
`cargo test -p sdkwork-api-cloud-gateway --lib vendor_open_api_auth` → **8 passed**；
`cargo fmt -- --check` 通过；门禁自测 7/7、本仓库判定通过。

**第二阶段（已接线，编译与单测通过）**：

- `runtime.rs`：IAM 解析器被 `wrap_gateway_api_key_resolver` 包装；`resolve_api_key`
  先查 `iam_gateway_api_key`（`key_hash` + `status=1` + `deleted_at IS NULL` +
  `revoked_at IS NULL` + 未过期），命中则投影 `WebRequestPrincipal`
  （tenant/org/user/apiKey/subjectType），未命中或无 PG/pepper 时原样委托平台解析器；
  其余 5 个凭证通道全部委托，不改变任何既有认证路径。
- `web_framework.rs::finalize_gateway_router{,_async}`：对 Cloud Router 的 open-api
  前缀安装 `vendor_credential_bridge`（`Authorization: Bearer` → `X-API-Key`）。
- 新增 9 条单测：哈希已知向量、pepper 键控、bridge 的 4 种分支、段级前缀匹配、
  无网关密钥时委托平台解析器。

验证：`cargo check -p sdkwork-api-cloud-gateway --lib` 通过；模块测试 **9 passed**；
`cargo fmt -- --check` 通过。**尚未部署**，因此线上行为未变；端到端验证需要一次
重建 + 用一个真实 `sk-` 网关密钥调用 `/v1/chat/completions`。

## 15. 登录 Auth Token 作为 API Key 的兼容收口（本轮实现）

第三阶段暴露了一个真实的接线缺口与一处凭证权威缺口。线上客户端
（BirdCoder 桌面端）按 vendor wire 只发一个 `Authorization: Bearer <IAM authToken>`，
因此每次请求都落在第 1 节的失败形态上；即使用 `sk-` 网关密钥，只要走
`Authorization: Bearer` 也同样被拒。

### 15.1 三处改动

1. **`runtime.rs::build_embedded_router` 安装 vendor 凭证桥**（接线缺口）。
   桥此前只挂在 `GatewayRuntime::router()` → `finalize_gateway_router` 上，
   而生产 embedded 组装直接调用 `build_embedded_router`，从未执行过桥。
   现在 `web_framework::attach_vendor_credential_bridge` 是桥的唯一所有者，
   `finalize_gateway_router{,_async}` 与 embedded 组装都调用它。
2. **API Key 通道按凭证形态分流，且只有一个所有者**。
   `sdkwork-iam` 的 `IamDatabaseWebRequestContextResolver::resolve_api_key` 负责分流：
   带 `sk-`/`sp-` 前缀只走 API Key 存储；无前缀凭证先查
   `iam_session.auth_token_hash`，查不到再落到 API Key 存储，因此既有的无前缀密钥与
   开发态 claim-string 不受影响。网关的 `GatewayApiKeyResolver` 只贡献
   `iam_gateway_api_key` 一个密钥源，其余原样委托 —— 一处实现，所有组装同构
   （平台网关 / 应用 standalone 网关 / 单 crate served router 不再各自为政）。
3. **`sdkwork-iam` 的 `resolve_bearer_auth_token` 真正解析登录 auth token**。
   该通道原本直接委托 `resolve_oauth_bearer`，只查 `iam_session.access_token_hash`，
   登录 `authToken`（`auth_token_hash`）永远查不到。现在先查 auth token，未命中再回落
   OAuth 通道，符合 `RouteAuth::OpenApiBearerFlexible` 的 trait 契约。

### 15.2 请求链路（修复后）

```
Authorization: Bearer <authToken>
  → attach_vendor_credential_bridge：改写为 X-API-Key，移除 Authorization
  → SurfaceClassification：api-key-or-dual-token 只看到 X-API-Key，通过
  → RequestContextResolution：resolve_api_key → 网关 iam_gateway_api_key 未命中
      → IAM resolve_api_key：非 sk-/sp- → iam_session.auth_token_hash
      → principal（tenant/org/user/app）
  → Cloud Router handler 从 X-API-Key 重新认证：分类器判为 AuthToken
      → IamAuthTokenAuthenticator → 默认上游账号分组（按 Chat/LLM 模态）
```

云侧不需要改动：`IamAuthTokenAuthenticator`、`authenticated_context_from_auth_token_identity`
与默认分组选择（`llm` 默认分组 → `mixed` 默认分组 → `default-group` 编码 → 唯一分组）
早已实现并有 e2e 覆盖。

**通道归属（本轮最终形态）**：`sk-`/`sp-` 与登录令牌的分流只由 IAM 的
`IamDatabaseWebRequestContextResolver::resolve_api_key` 负责，网关包装器只提供
`iam_gateway_api_key` 一个密钥源。这样平台网关、各应用 standalone 网关、单 crate
的 served router 对同一请求给同一答案，不会再出现"同一路由换个组装就换结论"。

### 15.3 验证

- `cargo test -p sdkwork-api-cloud-gateway --lib` → **77 passed**；其中
  `vendor_open_api_auth` 模块 **12 passed**（桥的 6 种分支：bearer 改写、Google 头改写、
  双令牌对保留、非 Bearer 不动、已有 `X-API-Key` 不动、空值不动；哈希已知向量；
  pepper 键控；段级前缀匹配；API Key 通道对所有凭证形态原样委托）。
- `cargo test -p sdkwork-api-cloud-gateway --tests` → 全绿（`web_framework_tests`
  **7 passed**：bearer→`X-API-Key`、非 vendor 路径不改写、embedded 组装确实装桥、
  shipped `component.spec.json` 的 `/v1` 前缀被桥覆盖）。
- `cargo test -p sdkwork-iam-web-adapter --lib` → **303 passed**（新增：无会话存储时
  通道拒绝回落到 API Key 通道、`sk-` 前缀永不进入会话通道）。
- `cargo test -p sdkwork-iam-web-adapter --test installation_ownership_scope_postgres
  -- --ignored`（需 `SDKWORK_DATABASE_URL` + 迁移）→ 新增
  `the_api_key_channel_resolves_a_login_auth_token_to_a_session_principal`：
  真实 `iam_session` 的 auth token 在 API Key 通道解析为会话 principal（无 `api_key_id`），
  且 `sk-` 凭证留在 API Key 通道。
- `cargo test -p sdkwork-iam-web-adapter --test open_api_resolver` → **3 passed**。
- `cargo fmt -- --check`（两个仓库）→ 通过。

### 15.4 上线前置条件

1. 重建并部署网关镜像（本轮改动在网关 + IAM web-adapter 两个 crate）。
2. `SDKWORK_API_CLOUD_GATEWAY_IAM_ALLOWED_AUDIENCES` 必须包含首方客户端的登录
   `app_id`（桌面端为 `birdcoder`），否则会话 principal 会被
   `IAM credential audience is not allowed for this application host` 拒绝；
   启动日志会列出缺失的已启用应用。
3. 租户必须存在 Chat 模态可用的默认上游账号分组（`llm` 默认分组或 `mixed` 默认分组），
   否则请求在依赖侧以 `503 account_group_unavailable` 失败（这是授权前置条件缺失，
   不是认证失败）。种子数据只为 `100_001` 租户建了 `mixed` 默认分组。
4. 客户端无需改动：`apiKeyEnv: BIRDCODER_API_KEY` 注入的正是登录 `authToken`，
   relay profile 的 `api: openai-completions` 指向 `/v1` 面，头为
   `Authorization: Bearer`。同一令牌在 `anthropic-messages` profile 下会以
   `x-api-key` 发到 `/anthropic/v1/messages`，本轮之后同样可认证（值分类不看头名），
   只是与 `/v1` 的 OpenAPI wire 不同形；客户端与网关文档里"Anthropic 面只接受 API Key"
   的旧描述已过时。

## 16. "Auth Token 与 API Key 同头传递" 的前后端完整性核对

要求：登录 Auth Token 必须与 API Key 走完全相同的 HTTP 头与传递方式。

### 16.1 客户端（sdkwork-birdcoder2）：一致，无需改动

- 两种凭证进入同一个 reference（`BIRDCODER_API_KEY`）与同一个 slot（pi-ai `apiKey`）；
  从 reference 到 header 的路径没有任何"来源分支"（`llm-pi-ai/src/index.ts` 只判断
  reference 是否存在），也没有针对会话头的额外 `Access-Token`／自定义头／query `key`。
- 同一协议下 header 逐字节相同：`openai-completions` / `openai-responses` →
  `Authorization: Bearer <value>`；`anthropic-messages` → `x-api-key: <value>` +
  `anthropic-version`。也就是说 header 形态由 **profile 的 `api`** 决定，而不是由
  "这是 token 还是 key" 决定 —— 这正是要求本身。
- 客户端不做 `sk-` 形状校验（`credentials/api-key.ts` 只做可打印 ASCII 与 trim），
  Models 页"已配置"只看凭证是否存在，不区分 key/token。
- 模型发现（"拉取模型"）用同一个 reference 与同一个头（`GET /v1/models`）。

### 16.2 服务端：四条链路全部同头同通道

| 层 | API Key | 登录 Auth Token |
| --- | --- | --- |
| 网关 edge（api.birdcoder.com） | `Authorization: Bearer sk-…` → 桥 → `X-API-Key` → `iam_gateway_api_key` | 同一桥、同一头 → 同一 `resolve_api_key` → IAM `auth_token_hash` |
| `X-API-Key` 直传（Anthropic face） | 不经过桥，直接 `iam_gateway_api_key` | 不经过桥，直接走登录令牌通道 |
| CloudRouter 调用面 | 值以 `sk-`/`sp-` 开头 → API Key 目录分组 | 其它值 → `IamAuthTokenAuthenticator` → **默认上游分组（按模态）** |
| IAM 解析器 | `iam_api_key` 记录 | `iam_session.auth_token_hash` |

- CloudRouter 侧 `/v1/*` 与 `/anthropic/*` 共用同一个 catch-all 鉴权
  （`invocation_router.rs` 通配路由 → `invocation_http.rs` → `authenticate_gateway_request_or_auth_token`），
  分类只看凭证**值**、不看头名，所以两种凭证在 `/v1/chat/completions`、`/v1/models`、
  `/v1/responses`、`/anthropic/v1/messages` 上行为一致。
- 网关的 vendor 前缀来自 shipped `component.spec.json`，`/v1` 与 `/anthropic/v1` 均在列；
  非凭证头（如 `anthropic-version`）不参与凭证校验，不会被拒。

### 16.3 本轮回归后新增的两条桥规则

- **双令牌对必须原样保留**：`api-key-or-dual-token` 的第二种合法形态正是
  `Authorization` + `Access-Token`。若桥把 bearer 半段改名成 `X-API-Key` 而留下
  `Access-Token`，框架会判为"mixed credential profile"并 400 —— 也就是说修 `/v1`
  单 bearer 的同时会打断文档里公开的双令牌调用。桥现在遇到 `Access-Token` 直接返回
  `DualTokenPreserved`，两个头都不动。
- **Google wire 的 `X-Goog-Api-Key` 也要桥**：框架 `STANDARD_CREDENTIAL_HEADERS`
  不含该头，`/google/v1beta` 客户端只发这个头时会被判成"无凭证"→ 401，API Key 与
  Auth Token 一起失败。桥现在把它同样改写为 `X-API-Key`，让 Gemini 兼容面与 `/v1`
  在同一凭证通道上对齐。

### 16.4 全量 open-api 回归（18 个 workspace / 45 个前缀）

按"同一凭证头、同一通道"逐 surface 核对的结果：

| 归类 | surface | 结论 |
| --- | --- | --- |
| 框架主渠道 | agents `/agent/v3/api`（56 ops，全部 `ApiKey`）、appstore 3 个 `ApiKey` op、IAM `provider_callbacks`、cloudrouter `/v1` 等 13 个前缀、drive/documents/knowledge/llm/prompts/community/cms/feeds/news/company/local-router | **对齐**：handler 只读框架注入 principal，不重读凭证；`sk-` 与登录令牌同头同通道 |
| 匿名面 | IAM `.well-known`/OAuth 端点/微信回调、appstore 5 个 `Public` op | **N-A（设计如此）**：`Public` 的凭证白名单为空，任何凭证头都 400，两种凭证完全同构 |
| 机器面 | cloudrouter `/internal/v3/gateway/*`（HMAC 签名） | **N-A（设计如此）**：永不接受用户会话 |
| 值形态边界 | 全部 | 以 `sk-`/`sp-` 开头的登录令牌会被判为 API Key；登录令牌为不透明串，属理论风险 |
| 残余 1 | IAM `GET /iam/v3/api/oauth/userinfo` | handler 只重读 `Authorization` 并只认 OAuth access token，**API Key 与登录令牌都失败**；这是契约（`[{ApiKey},{OAuthBearer}]`）与实现的偏差，与本轮凭证对齐无关，需单独决策 |
| 残余 2 | cloudrouter legacy relay posture | `/anthropic`、`/google`、`/kling`、`/minimax`、`/suno`、`/elevenlabs`、`/volcengine`、`/vidu` 的 provider-native 直通 handler 只认 API Key（仅未配 provider secret resolver 的旧部署） |
| 残余 3 | 部分应用 standalone 网关 | 未声明自身 open-api 前缀或仍接裸 IAM 解析器：前者两种凭证一起 401（组装缺陷），后者已由"通道归 IAM"修复 |
| 残余 4 | cloudrouter `/v1/vendors`、`/v1/user/balance` | 网关有路由但不在 open-api 契约与组合 manifest 中，两种凭证都 404 |
| 残余 5 | `?key=` 查询参数 | 网关框架不解析查询凭证；该形态仅在 standalone-desktop 策略下由依赖侧接受 |

结论：**所有由框架注入 principal 的 open-api 面已在同一凭证头、同一通道上与 API Key
完全对齐**；剩余项中没有任何一条是"同一凭证在 API Key 下成功、在登录令牌下失败"的
差异（残余 1 两者都失败，残余 2/3/4/5 与凭证类型无关）。








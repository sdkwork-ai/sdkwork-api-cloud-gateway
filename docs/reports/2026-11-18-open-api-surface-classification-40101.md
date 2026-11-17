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






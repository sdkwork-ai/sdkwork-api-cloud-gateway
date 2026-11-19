# 生成结果投递到 SDKWork Drive——存储中心配置体系设计

Date: 2026-11-19
Status: 设计待确认（P0）
Scope: sdkwork-cloudrouter（存储中心）、sdkwork-generations、sdkwork-image、sdkwork-voice、sdkwork-drive

## 1. 现状（代码证据）

| 组件 | 现状 | 证据 |
|---|---|---|
| CloudRouter 存储中心 | 已有 provider/bucket/default-bucket-policy/quota/usage/GC/reconciliation 七张治理表与 `/backend/v3/api/storage/*` 管理接口，种子在 `database/seeds/common/002_storage_center_defaults.sql` | `database/ddl/baseline/postgres/0001_cloudrouter_baseline.sql`、`crates/sdkwork-routes-cloudrouter-backend-api`、`services/sdkwork-cloudrouter-router-service/src/{api/admin_storage.rs,ports/admin_storage_store.rs,infrastructure/sql/postgres/admin_storage_store.rs}` |
| generations | `generation_result` 已有 `drive_space_id/drive_node_id/drive_uri` 字段，但五个适配器（image/video/music/sfx/voice）一律写 `drive_uri: None`，**没有 Drive 投递实现** | `crates/sdkwork-generations-provider-adapter/src/{image,video,music,sfx,voice}.rs`、`sdkwork-api-generations-assembly/src/bootstrap.rs:650-810` |
| image | 有 `ImageDriveImportRuntime`（生成后把输出导入 Drive 的运行时），宿主以 `Option<Arc<...>>` 注入；**没有开关、没有策略，注入即导入** | `crates/sdkwork-image-generation-runtime-service/src/drive_runtime.rs`、`crates/sdkwork-image-generation-host/src/service.rs:50,425` |
| voice | 有 `VoiceDriveSyncProcessor`，由进程级 `VOICE_DRIVE_SYNC_ENABLED` 控制 | `crates/sdkwork-voice-embedded-bootstrap/src/bootstrap.rs:53-66` |

结论：投递行为由**各仓库硬编码或进程环境变量**决定，没有统一策略、没有 vendor 过期处理、没有用户级设置、没有审计与配额联动。

## 2. 目标

1. 在**存储中心**统一配置"生成结果是否下载到 SDKWork Drive"以及下载到哪（空间/目录）、失败与过期如何处理。
2. 视频/图片/语音/音乐/音效统一遵循同一策略，生成服务不再各自硬编码。
3. vendor 产物有有效期：必须在过期前完成下载；过期未下载的结果可被识别、标记、告警与清理。
4. 作用域支持 tenant / organization / **user**，用户级覆盖优先（未来个性化），并可与存储配额联动。
5. 可审计、可灰度、可回滚（策略缺省时保持现有行为）。

## 3. 配置模型

### 3.1 策略对象

`GenerationDeliveryPolicy`：

| 字段 | 类型 | 说明 |
|---|---|---|
| scope_type | `tenant` \| `organization` \| `user` | 作用域类型（默认策略是隐式的全局行） |
| scope_id | string | 租户/组织/用户 id |
| media_kind | `all` \| `image` \| `video` \| `audio` | 可选的媒体维度，`all` 为整行默认 |
| delivery_mode | `to_drive` \| `remote_only` | 生成后是否下载到 Drive |
| drive_space_id | string? | 目标空间；空表示用户/租户默认空间 |
| drive_folder_node_id | string? | 目标目录节点；空表示空间根 |
| download_deadline_seconds | int? | 相对生成完成的下载截止（默认取 vendor TTL 的 80%） |
| vendor_expiry_action | `fail_job` \| `keep_reference` \| `delete_remote` | 过期未下载时的处理 |
| delete_remote_after_download | bool | 下载成功后是否删除 vendor 远端副本 |
| status | `active` \| `disabled` | 软停用 |

### 3.2 优先级与解析

解析顺序：`user + media_kind` → `organization + media_kind` → `tenant + media_kind` → 同作用域 `all` → 上一级作用域 → 全局默认。

- 取第一个命中行；缺失字段继承下一层（逐字段合并，不整行覆盖），避免"只改一个字段就得复制整行"。
- 解析结果带 `source_scope` 与 `resolved_from` 链，便于审计与 UI 展示来源。
- 无任何行时使用**系统默认**：`delivery_mode = remote_only`（**默认不落 Drive，节约存储与流量成本**，已确认）、`vendor_expiry_action = keep_reference`、`delete_remote_after_download = true`；只有显式配置了 `to_drive` 的作用域才把生成物下载到 Drive。

### 3.3 vendor 过期表

`VendorAssetTtl`：`vendor_code`、`media_kind`、`ttl_seconds`、`source`（默认表/上游声明/运营覆盖）、`status`。生成完成时按 vendor+kind 查表得到 `expires_at = finished_at + ttl`，写回生成结果；策略的 `download_deadline_seconds` 可缩短但不可超过 TTL。

## 4. 数据模型（CloudRouter 存储中心）

新增两张表（沿用 `storage_` 前缀与 scope 约定）：

```sql
CREATE TABLE storage_generation_delivery_policy (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL,
  organization_id BIGINT NOT NULL,
  scope_type TEXT NOT NULL,          -- tenant | organization | user
  scope_id TEXT NOT NULL,
  media_kind TEXT NOT NULL DEFAULT 'all',
  delivery_mode TEXT NOT NULL,       -- to_drive | remote_only
  drive_space_id TEXT,
  drive_folder_node_id TEXT,
  download_deadline_seconds INT,
  vendor_expiry_action TEXT NOT NULL, -- fail_job | keep_reference | delete_remote
  delete_remote_after_download BOOLEAN NOT NULL DEFAULT TRUE,
  status TEXT NOT NULL DEFAULT 'active',
  reason TEXT,
  created_at/updated_at TIMESTAMPTZ,
  CONSTRAINT ... CHECK (...)
);
CREATE UNIQUE INDEX uk_storage_generation_delivery_scope
  ON storage_generation_delivery_policy (tenant_id, organization_id, scope_type, scope_id, media_kind);

CREATE TABLE storage_vendor_asset_ttl (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL,
  organization_id BIGINT NOT NULL,
  vendor_code TEXT NOT NULL,
  media_kind TEXT NOT NULL,
  ttl_seconds INT NOT NULL,
  source TEXT NOT NULL,              -- default | upstream | operator
  status TEXT NOT NULL DEFAULT 'active',
  reason TEXT,
  created_at/updated_at TIMESTAMPTZ,
  CONSTRAINT ...
);
CREATE UNIQUE INDEX uk_storage_vendor_asset_ttl
  ON storage_vendor_asset_ttl (tenant_id, organization_id, vendor_code, media_kind);
```

生成侧数据（各服务自己的库）增加投递状态列：`delivery_mode`、`remote_url`、`assets_expires_at`、`drive_import_state`（`pending|importing|imported|skipped|expired|failed`）、`drive_space_id/drive_node_id`（generations 已有）、`delivery_policy_revision`。旧数据回填 `skipped`（保持历史行为）。

## 5. API

### 5.1 存储中心管理接口（`/backend/v3/api/storage/*`，沿用现有 admin 路由与权限模型）

- `GET /backend/v3/api/storage/generation_delivery_policies`（`generationDelivery.list`）
- `PUT /backend/v3/api/storage/generation_delivery_policies/{scopeType}/{scopeId}`（`generationDelivery.update`，按 `media_kind` 幂等 upsert）
- `GET /backend/v3/api/storage/vendor_asset_ttls`（`vendorAssetTtl.list`）
- `PUT /backend/v3/api/storage/vendor_asset_ttls/{vendorCode}/{mediaKind}`（`vendorAssetTtl.update`）

### 5.2 解析接口（按调用者身份解析，创建时快照）

`GET /app/v3/api/storage/generation_delivery?mediaKind=video&vendorCode=vidu`

- **为什么不用 internal-api**：CloudRouter 目前没有 internal-api surface（只有 app/backend/open），而解析必须按**目标租户/组织/用户**进行；用部署级机器密钥无法安全地替任意租户解析（会跨租户越权），新增 internal surface 又要重建一套清单/契约/SDK 链。app-api 天然携带登录用户的 `Authorization + Access-Token`，由 IAM 解析出 tenant/org/user，正好是策略作用域，且本会话已保证调用方双令牌透传。
- 生成服务在**创建生成时**（用户请求上下文内）调用一次，得到解析后的有效策略并**随生成记录持久化快照**（含 `source_scope`、TTL、截止时间）；后台完成/轮询任务只消费快照，不需要再次取用户凭证。
- 无用户上下文的纯后台流程（如设备轮次触发）使用系统默认（`remote_only`），并在记录中标记 `policySnapshotSource = default`。
- 内部一致性：解析实现只有一个（`AdminStorageStore::resolve_generation_delivery` + `domain/storage_delivery`），管理端列表/更新与 app-api 解析共用同一规则。

## 6. 生成侧集成

统一流程（generations/image/voice 共用）：

1. **创建生成时**（用户请求内）调用 §5.2 解析有效策略，并随生成记录持久化快照（含来源作用域、TTL、下载截止）。
2. 生成完成（vendor 返回资产 URL/bytes + 到期时间）后，后台任务消费该快照；没有快照（纯后台触发）时按系统默认（`remote_only`）。
3. `remote_only`：记录 `remote_url` 与 `expires_at`，状态 `skipped`；到接近过期仍未下载时按 `vendor_expiry_action` 处理（告警/保留引用/清理）。
4. `to_drive`：按目标空间/目录创建节点 → 分片上传（复用 image 的 Drive import 运行时）→ 回填 `drive_*`；成功后按 `delete_remote_after_download` 清理远端。
5. 失败：指数退避重试（受 `download_deadline_seconds` 约束），超过截止置 `failed` 并发出可观测事件；**绝不**在过期后伪造成功。
6. 幂等：以 `(generation_id, result_id)` 作为投递幂等键，重试不重复建节点。

落地顺序：
- **generations**：在完成/持久化结果的用例中插入投递步骤（当前是 `drive_uri: None`，改动集中在 service 完成路径 + 一个 `GenerationAssetDelivery` 端口）。
- **image**：把"注入 runtime 就导入"改为"策略决定"，保留兼容开关。
- **voice**：用策略替代 `VOICE_DRIVE_SYNC_ENABLED` 进程开关（环境变量保留为缺省值来源）。

## 7. vendor 产物过期处理

- `expires_at` 来源优先级：vendor 响应声明 → 运营覆盖表 → 内置默认表 → 保守默认（如 24h）。
- 生成结果投递状态机：`pending → importing → imported|skipped|expired|failed`。
- 定时任务（复用存储中心 GC 作业框架）：扫描 `skipped` 且 `expires_at` 临近的结果，触发最后一次下载尝试或按策略处置；过期资产在列表中标记不可下载。
- 告警/审计：策略变更、投递失败、过期未下载均写审计事件（存储中心已有审计约定，沿用）。

## 8. 用户级个性化

- `scope_type=user` 行可由用户自己的设置页写入（后续 UI），字段级继承组织/租户默认。
- 平台可设置"允许用户覆盖的字段白名单"（默认只允许 `delivery_mode`、`drive_space_id`、`drive_folder_node_id`），防止用户绕过租户的合规要求（如强制下载、禁止删除远端）。
- 租户可设置 `user_override_allowed = false` 冻结用户级设置。

## 9. 商业化要素

- 多租户隔离：所有读写按 `tenant_id + organization_id` 强制过滤（沿用存储中心约定）。
- 配额联动：投递前检查 `storage_quota_policy`/`storage_usage_counter`，超额时按策略标记 `failed` 并给出可读错误码。
- 幂等与并发：唯一索引 + 投递幂等键；策略更新走乐观版本（`updated_at`）。
- 可观测：投递成功率、过期率、平均下载耗时、失败原因分布；审计日志。
- 回滚：策略表与投递状态均为追加式，禁用策略即回到缺省行为；不需要数据回滚。

## 10. 迁移与兼容

- 默认策略 = `remote_only`（**已确认：默认不保存到 Drive，节约成本**）；租户/组织/用户显式配置 `to_drive` 后才下载，且策略缺省时与"仅保留 vendor 远端引用"的现状一致，不会静默改变行为。
- 旧结果回填 `skipped`；image 现有导入行为通过缺省策略保留。
- feature flag 控制"策略读取失败时回退到当前硬编码行为"，并在指标中暴露回退次数。

## 11. 待确认决策（附推荐）

1. **默认投递模式**：已确认 **`remote_only`**（生成物默认只保留 vendor 远端引用，不进入 Drive，节约存储与流量成本）；需要沉淀到 Drive 的租户/组织/用户显式配置 `to_drive`。
2. **Drive 目标默认**：推荐租户存储中心里"按媒体类型配置默认空间+目录"（`image/video/audio` 三行默认），用户可覆盖；空则落到用户默认空间根目录。
3. **过期未下载的默认动作**：推荐 `keep_reference`（保留远端引用 + 告警，不自动删除用户可能还想要的内容），运营可按租户改为 `fail_job`/`delete_remote`。

## 12. 分阶段交付与验收

| 阶段 | 内容 | 验收 |
|---|---|---|
| P1 | CloudRouter 存储中心：两张表 + 迁移/契约/种子 + admin 列表/更新 + 解析单元测试 | `cargo test`（router-service 存储域）+ 数据库契约检查 + 新迁移的 up/down 校验 |
| P2 | 内部解析接口 + generations 投递端口与完成路径（含幂等、失败重试、状态机） | generations 生命周期测试 + 本地 Drive 回环集成测试 |
| P3 | image/voice 接入策略（替换硬编码与进程开关）+ 兼容回退 | 两仓库工作区测试 + 回退路径测试 |
| P4 | vendor 过期扫描/告警/审计 + 用户级设置接口 + 配额联动 | 定时任务测试、审计断言、配额超额测试 |

每阶段独立可合并、可回滚；P1 不改变任何现有运行时行为。

## 13. 实现进度

| 阶段 | 状态 | 提交 |
|---|---|---|
| P1a 存储中心表 / 契约 / 解析规则 / 持久化 | 完成 | cloudrouter `af9874f6`；默认 `remote_only` `4575eb9e` |
| P1b 存储中心管理 API（策略 + vendor TTL） | 完成 | cloudrouter `03e10a60` |
| P2a app-api 解析端点（按调用者身份） | 完成 | cloudrouter `e9def02d`、`2b795dd2` |
| P2b-1 generations 投递快照表 + 领域契约 + 默认解析器 | 完成 | generations `da47de0` |
| P2b-2 解析客户端 + 创建时解析写快照 + 完成时应用（remote_only 记录 / to_drive 上传） | 待做 | — |
| P3 image / voice 接入策略 | 待做 | — |
| P4 vendor 过期扫描/告警、用户级设置、配额联动 | 待做 | — |

已确认默认：**不保存到 Drive（`remote_only`）**，只有显式配置 `to_drive` 的 tenant/organization/user 才下载。

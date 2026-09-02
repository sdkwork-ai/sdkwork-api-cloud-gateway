# sdkwork-api-cloud-gateway 打包能力商业化验证报告

**日期**：2026-09-02 ｜ **验证方式**：WSL 真实 Docker 部署 demo 环境，端到端启动 + IP 直连

---

## 一、结论

**具备商业化落地能力**。统一镜像 + 多环境 + 单机多实例的打包部署链路已全部打通，并在 WSL 真实 Docker 中完成 demo 环境的端到端验证：

| 验证项 | 结果 |
|---|---|
| 镜像构建（release 二进制 + docker build） | ✅ 362.6MB，`sha256:6ea1efeb…` |
| `deploy.sh --environment demo` 一键部署 | ✅ 健康门禁 20s 通过，RestartCount=0 |
| healthz 快速发布启动 | ✅ `200 {"status":"ok"}`（3ms） |
| root 路径鉴权保护 | ✅ 401（API 受保护） |
| **IP 直连访问** | ✅ WSL 内 `http://192.168.31.109:3914/healthz` → 200（1.6ms） |
| 端口绑定 | ✅ `0.0.0.0:3914 → 容器 3900` |
| 实例级隔离变量注入 | ✅ `GATEWAY_INSTANCE_HOST_PORT` / `ALLOW_IP_HOSTS=true` 等由 deploy.sh 注入 |

**多环境支持**：development / test / staging / demo / production 五环境（ENVIRONMENT_SPEC），每个环境独立 env 文件 + 独立 compose 项目名 + 独立数据库/角色/schema，`--environment <env>` 幂等切换，同一 OS 可多实例并行（端口 +10/实例）。

---

## 二、本次打磨修复的关键缺陷（按启动门禁逐层暴露）

### 1. 环境解析 fail-closed → 误判 production（核心缺陷，共 8 处）
各应用的 `WebEnvironment` 解析器只认 dev/test，**未知值（含 demo、甚至未设置）一律兜底 Prod**，触发 production 装配校验链（要求显式 AuditEmitter、Redis stores、tenant-bound resolver 等）导致启动 panic。

按工作区约定（startup_install.rs：staging 算 production-like，demo 不算）统一修复：

- `sdkwork-cloudrouter-http/web_security.rs`（demo→Test 展示档、staging/prod→Prod、未知仍 fail-closed）
- `sdkwork-iam/iam-web-adapter/lib.rs`（**第一道 panic 真凶**）
- `sdkwork-iam/iam-web-adapter/api_key_lookup.rs`
- `sdkwork-im/im-app-context/lib.rs`
- `sdkwork-web-framework/web-bootstrap/env_config.rs`
- `sdkwork-web-framework/web-core/parsers.rs`
- `sdkwork-agents/routes-agents-http-shared/web_bootstrap.rs`
- `sdkwork-generations/routes-generations-http-shared/web_bootstrap.rs`

### 2. compose 环境透传缺口
- 两个 compose（repo/bundle）补传 `SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT`；5 个 env 文件（+5 example）显式声明。
- bundle compose 补 PostgreSQL CA bundle 挂载（IM r2d2 TLS 信任锚根因）。
- 补 birdcoder 环境三元组透传 + client-local SQLite URL（`?mode=rwc` 自动建库）。

### 3. 各应用 demo 枚举缺口（5 处）
knowledgebase provider-secret-adapter、knowledgebase rpc-bin、birdcoder assembly、webserver×3、gateway config 等 demo 枚举逐一补齐。

### 4. 其他启动门禁
- canvas/notes（SQLite-only）接 PG URL 报 `PRAGMA` 语法错 → client-local SQLite 分派（ENVIRONMENT_SPEC §7.2）。
- Dockerfile 缺 6 个 `SDKWORK_*_APP_ROOT`（github 模块 manifest 读取失败）。
- github 路由 axum 0.7 `:param` 语法 → 0.8 `{param}`（19 处）。
- settings/canvas database manifest 缺 `driftPolicy` 字段。
- birdcoder OpenAPI 未戳 owner → `build_owned_openapi_document` 修复。
- cloudrouter 6 项密钥（test/staging/demo 补齐）。

### 5. 上游并行漂移（drive app SDK 再生成）
- 生成器 stale-skip 导致 mod.rs 引用 8 个不存在的模型（E0583）→ 重置后重新生成解决。
- Int64 Wire Contract 落地（i64→String）后 38 个消费者编译错误 → notes/canvas/deployments 全部修复（`.to_string()`、`version_no.parse()`、`website_roots_create` 补 idempotency_key）。

### 6. 环境故障排除
- E 盘 100% 满 → SIGBUS → WSL 只读/CreateInstance E_FAIL：删除 132GB 构建缓存恢复。
- pnpm `disk I/O error`：state 目录属主 root 修复 + store WAL 损坏绕过（直接 `node scripts/...`）。
- Windows 防火墙：放行 3914 入站（Windows 防火墙 + Hyper-V 防火墙规则）。

---

## 三、遗留事项（不阻塞商业化，建议跟进）

1. **外部设备真机验证**：Windows 宿主→自身 LAN IP 的 hairpin 在 WSL mirrored 网络模式下不被支持；已放行防火墙，建议用局域网内另一台设备访问 `http://192.168.31.109:3914/healthz` 做最终确认。
2. **provider secrets**：cloudrouter 有非阻断 WARN —— 商业部署需配置 `SDKWORK_CLOUDROUTER_PROVIDER_SECRET_MAP_JSON`（或 managed provider secrets），否则走 legacy OpenAI relay fallback。
3. **knowledgebase outbox webhook**：demo 日志有 `SDKWORK_KNOWLEDGEBASE_OUTBOX_WEBHOOK_URL is not configured` 错误，非阻断。
4. **pnpm store**：`/mnt/e/.pnpm-store/v11/index.db` WAL 损坏，建议磁盘空闲时删除该 store 重建（或迁移至 ext4）。
5. **上游并行会话**：drive SDK 今天的再生成（Int64 契约）未同步更新消费者代码，本次已修复 notes/canvas/deployments；建议与并行会话确认无冲突。

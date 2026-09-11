# 网关 Docker 打包与五环境全平面上线验证报告（2026-09-11 / 0.1.10）

## 结论

`registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.10`（imageId `sha256:bfb8c4671b4d…`，
364.3 MB）在 WSL Ubuntu-22.04 内从源码构建，以独立安装 bundle 打包，并在**两套平面**上完成
WSL 五环境部署：

| 平面 | 容器 | 镜像 | 结果 |
|---|---|---|---|
| 网关 bundle 舰队 | `sdkwork-api-cloud-gateway-<env>-i1-gateway-1` | `…:0.1.10` | 5/5 `healthy`，healthz/readyz 200 |
| 网关 bundle deps | `sdkwork-api-cloud-gateway-<env>-deps-knowledgebase-rpc-1` | `…:0.1.10` | 5/5 `healthy` |
| webserver 侧嵌入式网关（**公网平面活跃网关**） | `sdkwork-webserver-<env>-gateway-gateway-1` | `sdkwork-api-cloud-gateway:local` → `bfb8c4671b4d` | 5/5 `healthy`，`restarts=0` |

功能级 CORS 验收（**含最初报障路径**）：允许 7/7、拒绝 7/7，全部符合预期；五环境
`drift_errors=0`、`lease_unhealthy_hits=0`。

全程遵循 SDKWork 规范：镜像构建与部署均经 `bin/` 入口（`bin/docker-image.sh build`、
`bin/docker-deploy.sh upgrade`），侧栈沿用 webserver bundle 自身的 compose 调用方式
（`webserver/deployments/docker/bundle/deploy.sh:285`），未手敲 `docker build`。

## 一、交付物与门禁

| 项 | 值 |
|---|---|
| 镜像 | `registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.10` |
| imageId | `sha256:bfb8c4671b4d6a143ef61d90120ee3506abeb65fcd0365c52cac5ffaab13f660` |
| 尺寸 | 364.3 MB（本地 1.68 GB） |
| bundle | `dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.10.bundle/` |
| `image.tar.gz` sha256 | `bb57da616cbbb99763e4c4de7e0c4f2e642118aea6f51af0a0ec64f780d9b030` |
| 迁移 SQL 验收 | **107/107 IDENTICAL**（与 0.1.9 全量 `*.up.sql` sha256 比对） |

`107/107 IDENTICAL` 是漂移门禁的前置保障：`checksum_mismatch` / `migration_unknown`
属于启动即 fail-closed 的 error 级差异，字节等价即不会拒启。

版本口径唯一来源是 `sdkwork.app.config.json` 的 `release.currentVersion`（同时驱动镜像
tag、bundle 目录名、manifest 镜像 ref）；bump 到 0.1.10 后
`check-app-manifest-standard.mjs` 与 `check-app-manifest-deployment-standard.mjs
--strict-release-targets` 均通过。

## 二、两套平面：本轮的关键拓扑发现

排查中确认了一个此前未被记录的事实——**每个环境有两个网关平面，且共用同一个数据库**：

```
公网流量
  └─ webserver edge (Host: api-<env>.sdkwork.com, 端口 80/18898/18099/19098/18098)
        └─ 侧嵌入式网关  sdkwork-webserver-<env>-gateway-gateway-1   ← 公网平面的活跃网关
             同网络别名 sdkwork-api-cloud-gateway

直连/内部
  └─ 网关 bundle 舰队  sdkwork-api-cloud-gateway-<env>-i1-gateway-1 (宿主 3910/3911/3912/3914/3913)
        └─ deps/knowledgebase-rpc
```

决定性证据：webserver 应用容器只挂在 `sdkwork-webserver-<env>` 网络上，
`getent hosts sdkwork-api-cloud-gateway` 解析到 `172.27/28/29/30/31.x`，即**侧栈容器**。
两平面每环境共用同一库（`GATEWAY_POSTGRES_DB` = `SDKWORK_DATABASE_NAME` =
`sdkwork_ai_{dev,test,staging,demo,prod}`）。

> **由此推出的硬约束**：两平面必须使用**同一套** cloudrouter 密钥材料。pepper 不一致会让
> 一平面写入的 API key 哈希在另一平面无法校验；upstream credential key ring 不一致会让
> 一平面无法解密另一平面落库的上游凭据密文。

## 三、本轮修复的三个缺陷

### 1. 侧栈网关 crash-loop：cloudrouter 公开默认密钥触发 fail-closed 守卫

侧栈升级到 0.1.10 后容器 `Restarting (1)`，`restarts=18`：

```
Error: failed to build embedded router for `sdkwork-cloudrouter-app-api`:
  SDKWORK_CLOUDROUTER_API_KEY_PEPPER still uses published default secret material; …
  SDKWORK_CLOUDROUTER_TRUSTED_SUBJECT_SECRET still uses published default secret material; …
  SDKWORK_CLOUDROUTER_APP_SESSION_SECRET still uses published default secret material; …
```

根因：`sdkwork-cloudrouter/crates/sdkwork-cloudrouter-config/src/security_startup_guard.rs`
（提交 `364fa8ad`）对非 desktop 部署做**值级** secret 卫生检查，命中
`KNOWN_DEFAULT_SECRET_FRAGMENTS` 即拒启。而
`sdkwork-webserver/deployments/docker/docker-compose.bundle-gateway.yml` 仍把 5 个
`cloudrouter-local-dev-*-change-me` 占位值**硬编码**在 `environment:` 里——env 文件根本
无法覆盖（compose 里是字面量而非 `${}` 插值）。侧栈原镜像停留在 `0.1.5`
（`c2cb42af55c0`），早于该守卫，所以此前"能跑"。

修复分两步：

1. **compose 去物化**：5 个占位密钥 + key ring 默认值改为 fail-closed 注入
   `'${SDKWORK_CLOUDROUTER_<KEY>:?Required. Mirror GATEWAY_CLOUDROUTER_<KEY> from the co-deployed gateway bundle env.}'`，
   与 cloudrouter 自身 compose 的权威写法一致（`${CLOUDROUTER_API_KEY_PEPPER:?…}`）。
   `grep -c 'cloudrouter-local-dev' → 0`。
2. **env 镜像**：以网关 bundle env 的 `GATEWAY_CLOUDROUTER_*` 为单一真源，把 6 个键
   （`API_KEY_PEPPER` / `TRUSTED_SUBJECT_SECRET` / `APP_SESSION_SECRET` /
   `PAYMENT_WEBHOOK_SECRET` / `INTERNAL_GATEWAY_SIGNING_SECRET` /
   `UPSTREAM_CREDENTIAL_KEY_RING`）镜像为侧栈的 `SDKWORK_CLOUDROUTER_*`。

镜像前做了两道机器校验：

- 五环境 6×5 = 30 个源值**全部非空**（缺失即中止，不写任何文件）；
- 30 个值对 7 条 `KNOWN_DEFAULT_SECRET_FRAGMENTS` 的**子串命中数 = 0**（`frag-check.mjs`）。

这是**镜像而非轮换**：值与原 fleet 完全一致，因此不使既有 API key / 会话失效，
key ring 未变故既有上游凭据密文仍可解密。

### 2. 侧栈网关 `readyz=503`：Snowflake 节点租约被 fence

`/readyz` 报 `readiness_error=skills Snowflake node lease is unhealthy`，来自
`sdkwork-skills` 的 `SkillsReadiness::check_dependencies`。`node_allocator` 的
`is_healthy()` = 心跳任务仍在运行 **且** `lease_guard.allows(now)`；心跳一旦失败即
`lease_guard.fence()`，**进程内只分配一次、不会重试** → 永久不健康。

实测（demo）：`docker restart` 后 t=30s `{"status":"ready"}`，`unhealthy → healthy`。
即**重建容器即可恢复**，本轮 0.1.10 重发同时解决 CORS 与健康两个问题。
五环境升级后 `lease_unhealthy_hits=0`。

### 3. 构建阻断：`sdkwork-im` 工作树把 `unsafe_code` 定为 `forbid`

`-F unsafe-code` 并非来自 RUSTFLAGS（WSL 内未设置），而是 `sdkwork-im/Cargo.toml` 的
`[workspace.lints.rust] unsafe_code = "forbid"` 经 104 个成员 `[lints] workspace = true`
传递。该基线是**未提交工作树**（mtime 11:35），晚于 0.1.9 镜像的 09:36 构建，故 0.1.9 未受影响。

按 `RUST_CODE_SPEC.md` §6/§13（"A crate that genuinely needs `unsafe` changes it to
`deny` at the crate level"）与 `sdkwork-webserver/Cargo.toml` 先例，改为工作区级
`unsafe_code = "deny"` + 各 reviewed 站点 `#[allow(unsafe_code)]` + `// SAFETY:` + README
书面理由。`--keep-going` 全量枚举确认依赖图内仅 2 个 crate 含非测试 unsafe（
`sdkwork-im-service-readiness`、`sdkwork-api-im-assembly`），已全部覆盖。

## 四、部署矩阵

### 4.1 网关 bundle 舰队（`bin/docker-deploy.sh upgrade --image-tag 0.1.10`）

升级顺序按风险递增、逐环境三重门禁、**失败即停**（blast radius ≤ 1 环境）。

| 环境 | 宿主端口 | DEPLOY_EXIT | healthz | readyz | health | imageId |
|---|---|---|---|---|---|---|
| test | 3911 | 0 | 200 | 200 | healthy | `bfb8c4671b4d` |
| development | 3910 | 0 | 200 | 200 | healthy | `bfb8c4671b4d` |
| staging | 3912 | 0 | 200 | 200 | healthy | `bfb8c4671b4d` |
| demo | 3914 | 0 | 200 | 200 | healthy | `bfb8c4671b4d` |
| production | 3913 | 0 | 200 | 200 | healthy | `bfb8c4671b4d` |

五环境均在门禁 `t=5s` 即 healthz/readyz 双 200、`health=healthy`。staging/demo/production
触发升级前自动备份。因目标机已有 0.1.10 镜像，bundle 推送自动排除 `image.tar.gz`
（`ensure_image()` 命中即跳过 `docker load`）。

### 4.2 webserver 侧嵌入式网关

| 环境 | compose config | 旧镜像 | 新 imageId | health | restarts |
|---|---|---|---|---|---|
| test | ok | `c2cb42af55c0` (0.1.5) | `bfb8c4671b4d` | healthy | 0 |
| development | ok | `c2cb42af55c0` (0.1.5) | `bfb8c4671b4d` | healthy | 0 |
| staging | ok | `c2cb42af55c0` (0.1.5) | `bfb8c4671b4d` | healthy | 0 |
| demo | ok | `c2cb42af55c0` (0.1.5) | `bfb8c4671b4d` | healthy | 0 |
| production | ok | `c2cb42af55c0` (0.1.5) | `bfb8c4671b4d` | healthy | 0 |

执行顺序 test → development（试点并跑功能预检）→ staging → demo → production。
每环境先 `docker compose config -q` 校验（拦截插值/YAML 错误）再 `up -d`，
随后轮询 `health + healthz + readyz`。

> **端口陷阱**：侧栈网关在容器内监听 **8080**（实测旧容器 `3900 → 000`、`8080 → 200`）；
> 镜像 `EXPOSE 3900` 是过期元数据，compose healthcheck 走
> `${SDKWORK_MODULE_API_GATEWAY_UPSTREAM_PORT:-8080}/readyz` 才是准的。

### 4.3 侧栈运行态 CORS env

五环境均装配 `SDKWORK_CORS_ALLOWED_ORIGINS`（loopback 种子）+ 3 个
`SDKWORK_CORS_CONSOLE_HOST_{LABELS,SUFFIX,SCHEMES}`；生产 `SUFFIX=''`、`SCHEMES=https`。
客户端源站（`app://dsh|birdcoder|sdkwork|dtupay`、`tauri://localhost`、
`https://servicewechat.com`）由框架 `with_registered_sdkwork_client_origins()` 内置注入，
不再出现在 env 中。

## 五、功能级 CORS 验收

全部经**真实公网边缘**（webserver → 侧嵌入式网关），`OPTIONS /app/v3/api/oauth/scan_login_modes`：

允许（7/7）：

| 环境 | 端口 / Host | Origin | 结果 |
|---|---|---|---|
| dev | 80 / api-dev.sdkwork.com | `http://im-dev.sdkwork.com` | **200 + ACAO（最初报障路径）** |
| dev | 80 | `https://admin-dev.birdcoder.com` | 200 + ACAO |
| test | 18898 | `http://agents-test.sdkwork.com` | 200 + ACAO |
| staging | 18099 | `http://agents-staging.sdkwork.com` | 200 + ACAO |
| demo | 19098 | `http://portal-demo.sdkwork.com` | 200 + ACAO |
| prod | 18098 | `https://im.sdkwork.com` | 200 + ACAO |
| prod | 18098 | `https://admin.sdkwork.com` | 200 + ACAO |

拒绝（7/7，均**无** `Access-Control-Allow-Origin`）：

| 环境 | Origin | 拒绝理由 |
|---|---|---|
| dev | `http://evil.example.com` | 未注册基础域 |
| dev | `http://im.sdkwork.com` | 环境后缀错配 |
| dev | `http://im-dev.sdkwork.com.evil.com` | 后缀注入 |
| staging / demo | `http://im.sdkwork.com` / `http://im-dev.sdkwork.com` | 环境后缀错配 |
| prod | `http://im.sdkwork.com` | 生产仅允许 `https` |
| prod | `https://im-dev.sdkwork.com` | 生产不接受 `-dev` 后缀 |

负例全部命中，证明主机模式匹配**未退化为任意子域通配**：匹配仍要求单 DNS 标签、
无 userinfo/path/query、基础域在注册表内。

## 六、迁移前后体积（CORS 白名单）

`script` 级外科迁移（`migrate-deployed-cors.mjs` / `migrate-sidecar-cors.mjs`），
写入前均有硬断言：**任何旧源站若非"模式可覆盖 / 注册客户端源站 / loopback 种子"三者之一，
整体中止且不写文件**。

| 平面 | 环境 | 旧字节 | 新字节 | 旧条目 | 新条目 |
|---|---|---|---|---|---|
| bundle 舰队 | dev | 58052 | 42183 | 569 | 2 |
| bundle 舰队 | test | 21482 | 5053 | 569 | 2 |
| bundle 舰队 | staging | 23995 | 5870 | 569 | 2 |
| bundle 舰队 | demo | 21898 | 5489 | 568 | 2 |
| bundle 舰队 | prod | 13076 | 6419 | 297 | 2 |
| 侧栈 | dev | 24094 | 8282 | — | — |
| 侧栈 | staging | 29174 | 11106 | — | — |
| 侧栈 | prod | 15537 | 8937 | — | — |

五环境 fleet 迁移均输出 `coverage: all N legacy origins remain reachable`。

## 七、清理与归档

| 项 | 处置 |
|---|---|
| 侧栈 compose | 旧版本归档 `…/archives/<stamp>-sidecar-compose/`，再同步源侧 |
| 侧栈 env | 旧版本归档 `…/archives/<stamp>-cloudrouter-secrets/`（10 个文件） |
| 网关部署态 env 残留 | 4 个 `development.env.bak.20260908T17*` → `archives/legacy-env-bak-20260908/`，env 目录已 0 残留 |

回滚锚点：侧栈原镜像 `sdkwork-api-cloud-gateway:0.1.5`
（`sha256:c2cb42af55c0e1b9dff5cf83952b4d587b2d1cb004d665984ae5aad60fabe713`）仍有本地 tag。

## 八、复现命令

```bash
# 构建与打包
bash bin/docker-image.sh build --image-tag 0.1.10
pnpm build:container:install -- --skip-image-build

# 迁移 SQL 验收（与上一版全量比对）
bash .workbuddy/tmp/sql-acceptance-gate.sh

# CORS env 迁移（两平面；写入前有覆盖断言）
node .workbuddy/tmp/migrate-deployed-cors.mjs --apply
node .workbuddy/tmp/migrate-sidecar-cors.mjs --apply

# cloudrouter 密钥镜像（侧栈必须与 fleet 同源）
node .workbuddy/tmp/migrate-sidecar-secrets.mjs --apply

# 五环境升级（逐环境三重门禁，失败即停）
bash .workbuddy/tmp/upgrade-fleet.sh
bash .workbuddy/tmp/recreate-sidecar.sh test development staging demo production

# 验收
bash .workbuddy/tmp/verify-deploy.sh
bash .workbuddy/tmp/preflight-all.sh
```

## 九、遗留与待决策

1. **跨仓改动未提交**：`sdkwork-webserver`（compose 去物化 + CORS console 键映射 + 5 个源
   env）、`sdkwork-cloudrouter`、`sdkwork-im`（lint 基线）、`sdkwork-api-cloud-gateway`
   （manifest bump + 拓扑 env）以及 68 仓 CORS 门禁接线，均未提交 / 未推送。
2. **生产 cloudrouter 六件套**仍是 dev 占位密钥形态的历史项——本轮已确认真实值来自网关
   bundle env，但"是否应换为运维托管密钥"仍待决策。**注意**：轮换 key ring 必须把旧
   `activeKey` 移入 `decryptionKeys`，否则既有上游凭据密文不可解密。
3. **侧栈 compose 仍缺 `.env.example` 侧键**：本轮直接补进部署态与源侧 env；
   若后续要走"新机从 example 派生"，应在 `deployments/docker/env/*.env.example`
   同步这 6 个键（留空并注明生成方式），否则 `:?` 会在首次 `up -d` 时报错（这是预期 fail-closed 行为）。
4. **`SDKWORK_KNOWLEDGEBASE_OUTBOX_WEBHOOK_URL`** 在 staging/prod 仍缺失（每 10s 刷 ERROR，
   既有项非回归）。
5. **`bin/` 备份脚本 bug**：manifest 中 `database.dump.sha256` 为空仍去读。
6. **PG 18.4 `DROP DATABASE` 永久挂死**（`wait_event=IPC/ProcSignalBarrier`），残留探测库
   `sdkwork_ai_gateprobe` + 5 个历史探针库待确认后清理。

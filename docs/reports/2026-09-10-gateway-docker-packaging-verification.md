# 网关 Docker 打包与五环境上线验证报告（2026-09-10 / 0.1.9）

## 结论

`registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.9`（imageId `sha256:96bda1bcbd5b…`，
364.3 MB）在 WSL Ubuntu-22.04 内从源码构建完成，以独立安装 bundle 形式交付，并**原地升级
development / test / staging / demo / production 五个环境**，**5/5 通过健康门禁**
（容器内 healthcheck healthy、`/healthz` 与 `/readyz` 均为 200、公网平面 `api-<env>.sdkwork.com`
回 200、漂移门禁 `drift_fatal=0`）。

全程遵循 SDKWork 规范：所有镜像构建与部署均经 `bin/` 入口，未直接调用 `docker build` /
`docker compose`。部署态的 `env/<env>.env` 配置**原样保留**（deploy 日志逐项输出
`existing value kept; deploy never rewrites it`）。

## 一、构建链与交付物

| 步骤 | 命令 | 产物 |
|---|---|---|
| 镜像构建 | `bash bin/docker-image.sh build --image-tag 0.1.9` | `…:0.1.9` @ `sha256:96bda1bcbd5b…`，364.3 MB，耗时 267s |
| 安装 bundle | `pnpm build:container:install -- --skip-image-build` | `dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.9.bundle/` |

源码基线：`644a316`（`fix(release): resolve committed merge conflict in agent.app.config.json`）。

> **源码基线与并发改动说明**：镜像构建前的预检确认工作区**干净**（`git status --porcelain` 为空），
> 故 0.1.9 完全基于 `644a316`。构建完成后，工作区出现了**另一条工作流**的改动（Rust lint 基线：
> `[workspace.lints]` / `rust-version` / 各 crate `[lints] workspace = true`；CORS 单一真源重构：
> 新增 `scripts/cors-allowed-origins.mjs` 并同步多处 toml/env），这些改动**均发生在镜像构建之后**，
> **未进入 0.1.9**，本次未提交也未还原。若需要把 lint/CORS 变更带入制品，须在那些改动落地后
> **重新构建**。

bundle 内容与校验（`image.sha256` 校验通过）：

| 文件 | 大小 | sha256 |
|---|---|---|
| `image.tar.gz` | 380,708,066 B | `420deda6a90f22b50d3acf5ca2074a9a8fd4e885e7597c8ae692f01acbbff2ec` |
| `deploy.sh` | 41,625 B | `35e23e344d05ea38422df765cf17d8a2e1fe12a293aae9af4f03c7ec52f14105` |
| `manifest.json` | 667 B | `9a2b01827aed1575d1e79f9c8edc9f559fb7b3775bbc34afe18a83daafb94834` |
| `image.env` | 42 B | `289d98fad530ed561f5ff519d607807cd5ded02aab70c01ed31aaac1b33e8d2e` |

`bundle/env/` 仅含 5 个 `*.env.example`（不携带真实 env，故推送不会覆盖部署态配置）；
`compose/` 含 embedded / bundle / attach-override 三份编排；`deploy.sh`、`prepare-envs.sh`、
`release.sh` 均可执行且通过 `bash -n`。

## 二、本次修复的两个阻断缺陷

### 1. 打包迁移文件 CRLF 污染（fail-closed 漂移门禁）

五个环境的 `ops_schema_migration_history` 记录的 `sdkwork-appstore` 迁移校验和，
与工作区文件字节不一致：

| 文件 | 工作区（污染） | HEAD / 五库账本（正确） |
|---|---|---|
| `sdkwork-appstore/…/0002_appstore_platform_evolution.up.sql` | `45980ab1…`（442 CR） | `9e3fc9bf…`（LF） |
| `sdkwork-appstore/…/0003_user_store.up.sql` | `4c1860c9…`（85 CR） | `8b313c7c…`（LF） |

**三值判定**（工作区 = 镜像内 current；`git show HEAD:` = 五库 applied）证明这是**纯行尾漂移、
数据库本身正确**，因此修源码而非改数据库 —— 与既往「改数据库」路径相反，这一点已写入
skill `sdkwork-crlf-drift-gate-fix` 的「变体 B」章节。

反向枚举（421 个工作区 SQL 文件 × 账本校验和）确认**只有 2 个文件会触发门禁**；
DDL baseline / seed / `.down.sql` 不在门禁范围内（`sdkwork-database-drift/src/engine.rs:87,108`
只对 `list_migrations()` 的 `up_path` 取 sha256），故其余 43 个 CRLF 文件惰性无害。

`sdkwork-im/…/0005_im_outbox_events_retention_backfill.up.sql` 必须**保持 CRLF**（CR=40）——
账本记录的正是它的 CRLF 校验和。

### 2. 4 个迁移文件从工作区丢失 + 清单被提交了冲突标记

0.1.9 首次验收与 0.1.7 做全量 `*.up.sql` 对比时出现 **4 处删除**：`sdkwork-cloudrouter`
的 `0039/0040/0041` 与 `sdkwork-im` 的 `0005`。这些版本在**五库账本中都有对应行**，缺失会
让 `migration_unknown`（源码 `default_severity()` → `error`，两份 drift policy 均未覆盖）
直接 fail-closed 拒绝启动。已从 0.1.7 镜像恢复字节，与账本完全一致。

同时 `agent.app.config.json` 在 origin/main 上**带着未解决的 git 冲突标记**
（`<<<<<<< HEAD` / `=======` / `>>>>>>>`），导致任何构建都无法解析版本号
（`package-install-bundle.mjs` 报 JSON 解析错误）。已做并集合并并提交 `644a316`，
修正后 `currentVersion = 0.1.9`、零冲突标记。

### 验收门禁（构建后强制执行）

| 检查 | 结果 |
|---|---|
| 107 个打包 `*.up.sql` 的 CR 扫描 | 仅 `im/0005` 有 CR=40（应然） |
| 修复的 appstore 0002/0003 镜像内校验和 | `9e3fc9bf…` / `8b313c7c…` = 五库账本 |
| 与 0.1.7（已稳定运行）全量 sha256 对比 | **107/107 IDENTICAL** |

即 0.1.9 与 0.1.7 在漂移门禁的比对集合上**字节等价**。

## 三、五环境原地升级矩阵

升级顺序按风险递增，逐环境健康门禁、**失败即停**（blast radius 不超过一个环境）。

| 环境 | 升级前 | 升级后 | 容器端口 | healthz | readyz | docker |
|---|---|---|---|---|---|---|
| test | 0.1.7 | 0.1.9 | 3911 | 200 | 200 | healthy |
| development | 0.1.7 | 0.1.9 | 3910 | 200 | 200 | healthy |
| staging | 0.1.7 | 0.1.9 | 3912 | 200 | 200 | healthy |
| demo | 0.1.7 | 0.1.9 | 3914 | 200 | 200 | healthy |
| production | 0.1.7 | 0.1.9 | 3913 | 200 | 200 | healthy |

- 五环境 gateway 与 `deps/knowledgebase-rpc` 容器镜像 ID **均为 `96bda1bcbd5b`**。
- 公网平面（经 webserver attach）`Host: api-<env>.sdkwork.com` 全部 200：
  dev:80 / test:18898 / staging:18099 / demo:19098 / prod:18098。
- 因目标主机已有 0.1.9 镜像，bundle 推送**自动排除 `image.tar.gz`**（entrypoints.sh 行为），
  升级实际传输量很小；`deploy.sh ensure_image()` 先 `docker image inspect` 命中即跳过
  `docker load`，故部署态残留的旧归档不会被误加载。
- staging / demo / production 触发**升级前自动备份**（`sdkwork_backup_create`），
  例：`backups/sdkwork-api-cloud-gateway-production-20260911T032055Z`（含 config、data、
  postgres-data、postgres-tls、redis-data、secrets 及 `database.dump` 4.9 MB）。

耗时：test 40s 完成；五环境合计约 5.6 分钟。

## 四、门禁日志证据

每个环境的 `sdkwork_agents_database_host::drift_gate` 输出：

```
agents database schema passed startup drift gate with warnings
  module=agents drift_status=clean expected_tables=26 warning_diffs=1086 info_diffs=0
```

`drift_status=clean` 表示无 error 级差异（`extra_table` / `extra_column` 属 warn 级，
政策放行）；五环境 `checksum_mismatch` / `migration_unknown` / `missing_table` /
`missing_column` / `type_mismatch` 计数**均为 0**。

## 五、环境级问题与已知遗留（均非本次引入）

1. **PG 18.4 上 `DROP DATABASE` 永久挂死**（本次实测）：克隆库预演两次（各 15–31 分钟）
   都卡在 `DROP DATABASE IF EXISTS sdkwork_ai_gateprobe`，`pg_stat_activity` 显示
   `state=active`、`blocked_by={}`、**`wait_event=IPC/ProcSignalBarrier`** —— 即
   `PROCSIGNAL_BARRIER_SMGRRELEASE` 全局屏障未被某个后台 worker 满足，与连接占用、
   锁、prepared xact 均无关（均已排除）。**故克隆库预演在本实例不可行**；改为以
   **test 环境作为首个探路者**（真实库、真实配置、真实门禁，且失败即停），验证价值等价更强。
   五环境升级本身不涉及 CREATE/DROP DATABASE，不受此缺陷影响。
   残留 `sdkwork_ai_gateprobe` 库（79 MB，schema 已加载）待清理。
2. **升级前备份 manifest 小缺陷**：`database.dump` 的 `sha256` 为空，
   日志有 `bash: line 14: database.dump.sha256: No such file or directory` ——
   备份脚本对 `.sha256` 缺失未做存在性判断。不影响备份可用性。
3. **既有待决策告警**（0.1.7 时代即存在，非回归）：
   - `SDKWORK_KNOWLEDGEBASE_OUTBOX_WEBHOOK_URL` 未配置 → test/staging/demo/production
     的 outbox dispatcher 每 10s 刷 ERROR；
   - `SDKWORK_DEPLOY_SECRET_ENCRYPTION_KEY` 缺失 → 使用 development-only 派生密钥；
   - `SDKWORK_RTC_VOLCENGINE_*` 未配置 → RTC 仅信令模式；
   - `sdkwork_lr_store` 无加密密钥 → 上游 API key 明文存储。

## 六、新服务器快速上线（交付形态）

bundle 是自包含的，落地即可用：

```bash
# 1. 拷贝 bundle 到目标机（含 image.tar.gz 时可离线 load）
scp -r dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.9.bundle \
    user@new-host:/opt/deploy/sdkwork-api-cloud-gateway/
# 2. 准备环境配置（首次部署用 *.env.example 派生）
cd /opt/deploy/sdkwork-api-cloud-gateway/bundle
bash prepare-envs.sh --environment <env>
# 3. 部署（embedded 用于单机自包含；external 连接外部 PG/Redis）
bash deploy.sh --environment <env> --embedded
# 4. 健康检查
curl -s -H 'Host: gateway' http://127.0.0.1:<port>/healthz
```

规范入口同样可用（从仓库侧驱动远端主机）：

```bash
bash bin/docker-deploy.sh install --environment <env> --host ssh://user@new-host
```

## 七、复现命令

```bash
# 构建与打包
bash bin/docker-image.sh build --image-tag 0.1.9
pnpm build:container:install -- --skip-image-build

# 验收（与已知良好版本做 SQL 全量比对）
bash scripts/tmp/step8a-accept-019.sh

# 五环境升级（逐个环境健康门禁，失败即停）
bash scripts/tmp/step30-rollout.sh test
bash scripts/tmp/step30-rollout.sh development staging demo production

# 升级后深度验证
bash scripts/tmp/step31-post-verify.sh
```

注：`scripts/tmp/` 为本次一次性诊断脚本，交付后可整体删除。

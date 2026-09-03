# Docker 傻瓜式安装指南（docker-install）

> 版本：2026-09-03 · 适用：`sdkwork-api-cloud-gateway` standalone 统一安装镜像（bundle ≥ 0.1.1）
> 规范依据：`sdkwork-specs/DEPLOYMENT_SPEC.md` §6、`sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4
> English version: [docker-install.en.md](./docker-install.en.md)

## 0. 三条命令跑起来（TL;DR）

拿到安装包（`.bundle` 目录）后，在任意 Docker 主机上：

```bash
cp env/development.env.example env/development.env   # 1. 生成配置（默认值即可先冒烟）
bash deploy.sh --environment development --embedded  # 2. 部署（内置数据库，零外部依赖）
curl http://127.0.0.1:3910/healthz                   # 3. 健康检查，返回 ok 即成功
```

就这么多。**30 秒体验选上面这条路；要正式部署（尤其是 production），继续往下看
「分环境快速通道」（§0.1）**——production 有 7 个必填密钥和 1 个强制 TLS 开关，
漏了任何一项网关都会拒绝启动（fail-closed）。

### 0.1 分环境快速通道（复制即用）

每条通道假设你已经把 bundle 拷到目标机并 `cd` 进 bundle 目录（§3）。

**development（开发，内置依赖，零配置）：**

```bash
cp env/development.env.example env/development.env
bash deploy.sh --environment development --embedded
curl http://127.0.0.1:3910/healthz
```

**test（测试，内置依赖，零配置）：**

```bash
cp env/test.env.example env/test.env
bash deploy.sh --environment test --embedded
curl http://127.0.0.1:3911/healthz
```

**staging（预发，外置数据库；内置密钥为 dev 默认值，仅限内部预发）：**

```bash
cp env/staging.env.example env/staging.env
# 编辑 env/staging.env：填 GATEWAY_POSTGRES_HOST/PORT/DB/USER/PASSWORD 指向你的外部库
bash deploy.sh --environment staging            # 外置模式（默认）
bash deploy.sh --environment staging --embedded # 或内置模式快速起
curl http://127.0.0.1:3912/healthz
```

**production（生产，7 个密钥 + 强制 TLS，见 §4.1 一键生成）：**

```bash
cp env/production.env.example env/production.env
# ① 替换 7 个 <CHANGE_ME>（1 个数据库密码 + 6 个 Cloud Router 密钥，§4.1 有生成命令；
#    本地验证可偷懒：bash prepare-envs.sh --dev-secrets-for-production）
# ② 外置模式：填 GATEWAY_POSTGRES_* 连接信息（sslmode=require 示例已预置）
bash deploy.sh --environment production --replicas 3   # 或加 --embedded
curl http://127.0.0.1:3913/healthz
```

> 同一台机器同时跑多个环境？完全可以（端口/网络/卷/库名天然隔离），但必须给每个环境
> 设**不同的** `GATEWAY_IM_ID_NODE_ID`，见 §6 矩阵表。

## 1. 两种依赖模式：怎么选

一个镜像、两种姿态，部署时用开关选择；**默认外置**：

| | 外置模式（默认） | 内置模式（`--embedded`） |
| --- | --- | --- |
| PostgreSQL | 自备（托管实例或自建），`GATEWAY_POSTGRES_*` 指过去 | 随包自动启动（pgvector/pgvector:pg18，含 vector 扩展，自动 TLS） |
| Redis | 自备，`GATEWAY_CLOUDROUTER_REDIS_*` 指过去 | 随包自动启动（redis:8-alpine，无密码） |
| 适用场景 | 测试 / 预发 / 生产，有 DBA 或托管数据库 | 快速演示、PoC、隔离冒烟测试、无外部数据库的小规模部署 |
| 数据位置 | 在你的数据库实例里 | 在宿主机 Docker 卷 `<环境>-postgres-data` / `<环境>-redis-data` |
| 升级 / 扩容 | 无状态，随时 `--replicas N` | 同样无状态；内置库单实例，是容量上限 |

规则很简单：**有像样的 PostgreSQL 和 Redis 就用外置（默认）；没有或只想快速看效果就 `--embedded`。** 两种模式用同一个镜像、同一份 env 文件，切换只是加/去一个参数，改完重新执行 `deploy.sh` 即可。

> 内置模式仍按环境隔离：每个环境一套独立网络、独立数据库卷、独立库名（`sdkwork_ai_dev/test/...`），互不影响。
> 内置模式的 PostgreSQL 同样启用 TLS（证书自动生成）；连接串 sslmode 跟随
> `GATEWAY_POSTGRES_SSL_MODE`（production/staging 示例已预置 `require`）。

## 2. 第一步：打包安装包（构建机）

在仓库根目录一条命令产出自包含安装包（镜像 + compose + env 模板 + 部署脚本）：

```bash
pnpm build:container:install                        # 构建镜像 + 打包
pnpm build:container:install -- --skip-image-build  # 复用已构建镜像，只重新打包
pnpm build:container:install -- --dry-run           # 只看计划，不实际执行
```

产物布局（版本号取自 `sdkwork.app.config.json`，当前为 0.1.1）：

```text
dist/docker-install/sdkwork-api-cloud-gateway-install-<版本>.bundle/
├── image.tar.gz / image.sha256 / image.env   # 镜像归档 + 校验 + tag
├── compose/
│   ├── docker-compose.bundle.yml             # 环境中立多实例模板（外置依赖，默认）
│   └── docker-compose.bundle.embedded.yml    # 内置 PostgreSQL/Redis 覆盖（--embedded 用）
├── postgres/                                 # 内置模式的 postgres 初始化与 TLS 资产
├── env/                                      # 5 个环境的配置模板
│   ├── development.env.example  test.env.example  staging.env.example
│   └── demo.env.example  production.env.example
├── deploy.sh                                 # 唯一部署入口
├── prepare-envs.sh                           # 一键生成各环境 env（节点 ID 自动隔离）
├── manifest.json                             # 版本 / 镜像 / sha256 元数据
└── README.md
```

## 3. 第二步：拷贝到目标机

把整个 `.bundle` 目录拷贝到目标 Linux 主机（scp / rsync / U 盘均可），要求只有两个：
**装好 Docker（含 compose 插件）**、**能访问镜像仓库或已随包携带 image.tar.gz**。

离线安装（无任何外网）：先校验再部署——

```bash
sha256sum -c image.sha256    # 必须输出 image.tar.gz: OK
```

不想手动拷？跳到 §8 远程一键部署。

## 4. 第三步：配置（一个文件搞定）

```bash
cp env/<环境>.env.example env/<环境>.env
```

首次执行 `deploy.sh` 时如果 env 文件不存在，也会自动从 example 生成。**必须关注的字段**（其余字段默认值即可）：

| 字段 | 说明 | 外置模式 | 内置模式 |
| --- | --- | --- | --- |
| `GATEWAY_POSTGRES_HOST/PORT/DB/USER/PASSWORD` | 外部 PostgreSQL 连接信息 | **必填** | 可忽略 |
| `GATEWAY_CLOUDROUTER_REDIS_HOST/PORT/PASSWORD` | 外部 Redis 连接信息 | **必填** | 可忽略 |
| `GATEWAY_POSTGRES_SSL_MODE` | production/staging 示例已预置 `require`（P0-12 强制：生产库连接必须 TLS） | 生产**必填** | 生产**必填**（内置库 TLS 自动就绪） |
| `GATEWAY_DEPS_MODE` | `external`（默认）或 `embedded` | 不用动 | 可写 `embedded` 代替 `--embedded` |
| `GATEWAY_IM_ID_NODE_ID` | 雪花 ID 节点号；**同机多环境必须互不相同**（§6） | 建议修改 | 建议修改 |
| `GATEWAY_CLOUDROUTER_API_KEY_PEPPER` 等 6 个密钥 | production 环境占位符必须替换 | 生产必填 | 生产必填 |

**密码包含 URL 特殊字符时**：不要直接填 `GATEWAY_POSTGRES_PASSWORD`，改用百分号编码后的完整
`SDKWORK_DATABASE_URL`（示例文件里有说明注释）。注意：env 文件里显式设置了
`SDKWORK_DATABASE_URL` 时它优先生效——内置模式下请删除该行或改为指向 `postgres:5432`。

> 提示：env 模板中的 `<CHANGE_ME>` 占位符如果原样保留，会**字面成为**生效的密码/密钥
> （功能上能跑通，但等同于弱凭据）。冒烟无所谓，production 一定要替换（§4.1）。

外置模式的数据库与 schema 需按环境预先建好（`sdkwork_ai_dev/test/staging/demo/prod`），
仓库内可执行 `pnpm db:postgres:provision` 一键创建。

### 4.1 production 密钥一键生成（傻瓜式）

production 模板里有 **7 个 `<CHANGE_ME>`**。在目标机（需 openssl，Docker 主机都有）逐条执行，
把输出粘进 `env/production.env` 对应行即可：

```bash
# ① 数据库密码（外置模式；同时要在数据库侧改掉）
openssl rand -hex 24

# ② 五个随机密钥：API_KEY_PEPPER / TRUSTED_SUBJECT_SECRET /
#    APP_SESSION_SECRET / PAYMENT_WEBHOOK_SECRET / INTERNAL_GATEWAY_SIGNING_SECRET
#    （每个密钥各执行一次，粘到对应行）
openssl rand -hex 32

# ③ UPSTREAM_CREDENTIAL_KEY_RING（整行替换为下面命令的输出，一行完整 JSON）
echo "{\"activeKeyId\":\"prod-v1\",\"activeKey\":\"$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')\",\"fingerprintKey\":\"$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')\",\"decryptionKeys\":[]}"
```

> KEY_RING 的 `activeKeyId` 自定义为 `prod-v1` 即可；**上线后不要更换已发布的
> activeKey**（会导致已签发凭据失效），轮换时把旧 key 挪进 `decryptionKeys`。

一条命令检查还有没有漏网的占位符（应无输出）：

```bash
grep -n "<CHANGE_ME>" env/production.env
```

staging 的 6 个 Cloud Router 密钥模板已带 dev-only 默认值（仅限内部预发），正式对外预发
请按 §4.1 同样方式替换。

## 5. 第四步：部署与验证

`deploy.sh` 是唯一入口，幂等可重复执行：

```bash
bash deploy.sh --environment development                  # 开发环境，单实例，外置依赖
bash deploy.sh --environment development --embedded       # 同上，内置 PostgreSQL + Redis
bash deploy.sh --environment production --replicas 3      # 生产 3 实例（实例 1 负责迁移）

bash deploy.sh --environment test --ps                    # 看状态
bash deploy.sh --environment test --logs 1                # 跟踪实例 1 日志
bash deploy.sh --environment test --down                  # 停止（保留数据卷）
bash deploy.sh --environment test --down --purge          # 停止并删除卷/网络（内置库数据也会删除！）

bash deploy.sh --help                                     # 全部参数
```

部署顺序由脚本自动编排：内置库（如启用）→ `knowledgebase-rpc` 单例（预置共享密钥/mTLS 材料）→
实例 1（执行数据库迁移）→ 实例 2..N（跳过迁移，待实例 1 健康后启动）。
首次全新部署（含内置库初始化 + 全量迁移）最长约 18 分钟属正常；重复部署幂等且很快。

**验证**：

```bash
curl http://127.0.0.1:<实例端口>/healthz    # 存活
curl http://127.0.0.1:<实例端口>/readyz     # 就绪（首次启动需等迁移完成，最长约 4 分钟）
```

在 Windows 上通过 WSL/ Docker Desktop 部署时，Windows 浏览器直接访问
`http://localhost:<实例端口>/healthz` 即可（已验证互通）。注意本机配置了 HTTP 代理时，
curl 验证请加 `--noproxy '*'`，否则结果可能被代理污染。

## 6. 多环境矩阵（端口 / 节点 ID / 数据库规划）

实例端口公式：`实例 i 端口 = GATEWAY_HOST_PORT + (i-1) × 10 → 容器 3900`。
同一台机器可同时跑多个环境，端口、网络、卷、数据库天然隔离；**但雪花 ID 节点号
`GATEWAY_IM_ID_NODE_ID` 必须每环境不同**，否则同机多舰队可能生成重复 ID：

| 环境 | 基础端口（实例 1） | 建议节点 ID | 数据库 / Schema | Redis 逻辑库 | 示例端口（3 实例） |
| --- | --- | --- | --- | --- | --- |
| development | 3910 | 21 | `sdkwork_ai_dev` | 1 / 2 | 3910 / 3920 / 3930 |
| test | 3911 | 22 | `sdkwork_ai_test` | 3 / 4 | 3911 / 3921 / 3931 |
| staging | 3912 | 23 | `sdkwork_ai_staging` | 5 / 6 | 3912 / 3922 / 3932 |
| production | 3913 | 24 | `sdkwork_ai_prod` | 7 / 8 | 3913 / 3923 / 3933 |
| demo | 3914 | 25 | `sdkwork_ai_demo` | 9 / 10 | 3914 / 3924 / 3934 |

每个环境的配置集中在一份 env 文件（所有实例共用）：bundle 内 `env/<环境>.env`，或
远程 Ubuntu 的 `/etc/sdkwork/api-gateway/docker/<环境>.env`。需要单实例差异化时，
创建 `env/<环境>.i<序号>.env` 会自动叠加。

公网反向代理由独立的 `sdkwork-webserver` 部署持有：把各环境域名上游指向该环境的实例端口列表
即可（如生产 `api.{sdkwork,birdcoder,dtupay}.com` → `127.0.0.1:3913; 3923; 3933`）。
网关侧域名 allowlist / CORS 已在 env 示例中按环境预置。

**同机多环境一键重建配置**：bundle 内置 `prepare-envs.sh`，一条命令生成全部环境的
env 文件——自动按 §6 表分配互不相同的节点 ID，已存在的 env 一律不覆盖（升级重跑安全）：

```bash
bash prepare-envs.sh                               # 生成缺失的 env/<环境>.env（幂等）
bash prepare-envs.sh --dev-secrets-for-production  # 本地验证用：production 6 密钥以 dev 默认值填充（严禁真实生产）
bash prepare-envs.sh --force                       # 重置为示例值（丢弃手工修改，慎用）
```

## 7. 内置模式细节

- **启动内容**：deps 项目里多两个容器——`postgres`（pgvector 镜像，`ssl=on`，证书自动生成，
  schema 自动创建）与 `redis`（无密码）。网关与 knowledgebase-rpc 自动指向它们，无需改任何
  `GATEWAY_POSTGRES_*` / `GATEWAY_CLOUDROUTER_REDIS_*`。
- **数据持久化**：数据在 Docker 卷 `<环境>-postgres-data` / `<环境>-redis-data`，
  `--down` 保留、`--down --purge` 删除。备份：`docker run --rm -v <环境>-postgres-data:/data -v $PWD:/backup alpine tar czf /backup/pg.tar.gz /data`。
- **迁回外置**：先用 pg_dump 从内置库导出，再在外部 PostgreSQL 导入，最后去掉
  `--embedded` 重新执行 `deploy.sh` 并把 `GATEWAY_POSTGRES_*` 指向新实例。
- **限制**：内置库是单实例（无主从/集群），不承担生产级容量与高可用；生产环境请用外置。
- production / staging 内置模式同样强制数据库 TLS：示例已预置
  `GATEWAY_POSTGRES_SSL_MODE=require`，内置库的 TLS 由 compose 自动配置，无需手工干预。
- 若 env 文件里显式设置了 `SDKWORK_DATABASE_URL`，它优先生效——内置模式下请删除该行或改为
  指向 `postgres:5432`。

## 8. 远程 Ubuntu 一键部署（SSH 推送）

本地打包后直接推到远程主机部署，配置以远程为准、重复执行不覆盖：

```bash
# 前置：ssh-copy-id deploy@1.2.3.4 一次（免密登录）
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --environment production --replicas 3

# 内置依赖的远程部署
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --environment development --embedded

# 常用：复用已有 bundle / 指定密钥 / 预演
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.1.bundle \
    --ssh-key ~/.ssh/prod_ed25519 --environment test --replicas 2 --dry-run
```

远程配置文件：`/etc/sdkwork/api-gateway/docker/<环境>.env`（权限 0600，首推自动生成）。
改密码 / 改数据库 → 编辑该文件 → 重新执行同命令即生效。需要远程 sudo 免密（`sudo -n`），
否则用 `--config-root` 指向用户可写目录。

## 9. 排障速查表 + FAQ

### 9.1 症状速查（先对表，再看 FAQ）

| 症状 | 最可能原因 | 处置 |
| --- | --- | --- |
| 容器 `Restarting` 循环，日志有 `must contain sslmode=require` | production/staging 强制数据库 TLS（P0-12 fail-closed），连接串是明文 | 确认 env 里 `GATEWAY_POSTGRES_SSL_MODE=require`（外置模式指到支持 TLS 的库；内置模式模板已自动处理）；改完重新 `deploy.sh` |
| 容器 `Restarting` 循环，日志有 `is required for ...` | 对应 fail-closed 密钥未配置（如 6 个 Cloud Router 密钥） | 补齐 env 中缺失密钥（§4 / §4.1），重新部署 |
| `port is already allocated` | 端口被旧舰队或其他环境占用 | `docker ps --format '{{.Names}}\t{{.Ports}}'` 找占用者；换 `GATEWAY_HOST_PORT` 或清理旧部署 |
| `/healthz` 一直不通但容器 healthy | 本机代理劫持了 curl | `curl --noproxy '*' http://127.0.0.1:<端口>/healthz` |
| 部署命令"成功"但容器没起来 | 管道吞了退出码（假通过） | 部署后始终跑一次 `curl /healthz` 实证，不要只看命令退出码 |
| 多实例只有实例 1 起来 | 实例 1 迁移未完成，2..N 在等 | 等 `/readyz` 200（最长约 4 分钟），其余实例自动跟上 |
| `docker load` 后镜像 tag 不对 | 手工导入 OCI 归档未打 tag | `docker tag <imageID> <manifest.json 里的 ref>` |

### 9.2 FAQ

**Q：`curl /healthz` 不通？**
按顺序查：`bash deploy.sh --environment <env> --ps` 看容器是否 healthy →
`--logs 1` 看启动日志（崩溃循环对着 §9.1 速查表找原因）→ 外置模式先确认
`GATEWAY_POSTGRES_HOST` 从容器内可达（宿主机上跑的库用 `host.docker.internal`，env 示例默认值即是）→
首次启动迁移耗时较长，`/readyz` 最长等 4 分钟。

**Q：数据库密码怎么改？**
改 env 文件里的 `GATEWAY_POSTGRES_PASSWORD`（或整条 `SDKWORK_DATABASE_URL`）→
重新执行 `deploy.sh --environment <env>`。注意数据库侧的密码要同步修改。

**Q：怎么扩容 / 缩容？**
提高或降低 `--replicas N` 重新执行即可（幂等）。新实例自动跳过迁移；缩容只是停掉多余实例项目。

**Q：`--down --purge` 删了什么？**
环境的网络 + secrets/data 卷；内置模式下还会删除内置库数据卷（**不可恢复**）。外置模式不碰你的数据库。

**Q：多个环境能装在同一台机器上吗？**
可以。端口、网络、卷、数据库名全部按环境隔离，参考 §6 矩阵表；记得给每个环境设不同的
`GATEWAY_IM_ID_NODE_ID`。

**Q：镜像没有仓库可推？**
`image.tar.gz` 就是镜像本体，deploy.sh 检测到镜像缺失会自动 `docker load`，完全离线可用
（离线部署前先 `sha256sum -c image.sha256` 校验，见 §3）。

**Q：升级到新版本安装包怎么做？**
数据在外置库或 Docker 卷里，升级不丢数据：`--down` 停旧版 → 用新 bundle 重新 `deploy.sh` →
`/healthz` 验证。镜像有更新时 deploy.sh 会按 `image.env` 的 tag 自动 load 新镜像。

## 10. 运维检查清单

- [ ] `manifest.json` 中 image sha256 与 `image.sha256` 一致（离线部署另做 `sha256sum -c image.sha256`）
- [ ] 每个环境 `/healthz` 与 `/readyz` 通过（用 `--noproxy '*'` 排除代理干扰）
- [ ] 多实例部署下仅实例 1 出现迁移日志
- [ ] 同机多环境的 `GATEWAY_IM_ID_NODE_ID` 互不相同
- [ ] 生产 / 预发 env 中所有 `<CHANGE_ME>` 占位已替换（1 个数据库密码 + 6 个 Cloud Router 密钥；`grep -n "<CHANGE_ME>" env/<环境>.env` 应无输出）
- [ ] 生产 / 预发 `GATEWAY_POSTGRES_SSL_MODE=require`（外置与内置模式都要求，P0-12）
- [ ] `SDKWORK_DATABASE_URL` 显式覆盖与当前依赖模式一致（内置模式应为 `postgres:5432` 或删除）
- [ ] sdkwork-webserver 各环境域名经反代可达网关 API

完整操作手册：`docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md`；
Ubuntu 裸机（systemd）安装见 `docs/guides/operator/INSTALLATION_STANDARDS.md`。

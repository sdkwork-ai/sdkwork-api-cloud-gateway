# Docker 傻瓜式安装指南（docker-install）

> 版本：2026-09-03 · 适用：`sdkwork-api-cloud-gateway` standalone 统一安装镜像（bundle ≥ 0.1.1）
> 规范依据：`sdkwork-specs/DEPLOYMENT_SPEC.md` §6、`sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4
> English version: [docker-install.en.md](./docker-install.en.md)

## 0. 三条命令跑起来（TL;DR）

拿到安装包（`.bundle` 目录）后，在任意 Docker 主机上。**还没有安装包？第一步永远是
打包——见 §2（含重新打包的场景选择 §2.1）**。**默认外置依赖**：
数据库和 Redis 用你自己的实例，网关通过 env 里的 `GATEWAY_POSTGRES_*` /
`GATEWAY_CLOUDROUTER_REDIS_*` 连过去：

```bash
cp env/development.env.example env/development.env   # 1. 生成配置
# 编辑 env/development.env：填 GATEWAY_POSTGRES_* 与 GATEWAY_CLOUDROUTER_REDIS_*
# 指向你的外部 PostgreSQL 和 Redis（连接项说明见 §1 / §4）
bash deploy.sh --environment development             # 2. 部署（默认外置模式）
curl http://127.0.0.1:3910/healthz                   # 3. 健康检查，返回 ok 即成功
```

**正式部署（尤其是 production）继续看「分环境快速通道」（§0.1）**——production 有
7 个必填密钥和 1 个强制 TLS 开关，漏了任何一项网关都会拒绝启动（fail-closed）。
没有现成的 PostgreSQL/Redis、只想 30 秒冒烟？把第 2 步换成
`bash deploy.sh --environment development --embedded`（随包内置库，零外部依赖）。

### 0.1 分环境快速通道（复制即用）

每条通道假设你已经把 bundle 拷到目标机并 `cd` 进 bundle 目录（§3）。
**默认外置模式**：先在 env 里填外部数据库/Redis 连接信息，再 `deploy.sh`；
没有外部库时加 `--embedded` 用随包内置库替代。

**development（开发，默认外置依赖）：**

```bash
cp env/development.env.example env/development.env
# 编辑 env/development.env：填 GATEWAY_POSTGRES_* / GATEWAY_CLOUDROUTER_REDIS_*（§4.2 有完整步骤与示例）
bash deploy.sh --environment development             # 外置模式（默认）
bash deploy.sh --environment development --embedded  # 没有外部库时：内置模式，零配置
curl http://127.0.0.1:3910/healthz
```

**test（测试，默认外置依赖）：**

```bash
cp env/test.env.example env/test.env
# 编辑 env/test.env：填 GATEWAY_POSTGRES_* / GATEWAY_CLOUDROUTER_REDIS_*（§4.2 有完整步骤与示例）
bash deploy.sh --environment test                    # 外置模式（默认）
bash deploy.sh --environment test --embedded         # 或内置模式快速起
curl http://127.0.0.1:3911/healthz
```

**staging（预发，外置数据库；内置密钥为 dev 默认值，仅限内部预发）：**

```bash
cp env/staging.env.example env/staging.env
# 编辑 env/staging.env：填 GATEWAY_POSTGRES_HOST/PORT/DB/USER/PASSWORD 指向你的外部库
# （外置 PostgreSQL 与 Redis 的完整配置步骤与示例见 §4.2）
bash deploy.sh --environment staging            # 外置模式（默认）
bash deploy.sh --environment staging --embedded # 或内置模式快速起
curl http://127.0.0.1:3912/healthz
```

**production（生产，7 个密钥 + 强制 TLS，见 §4.1 一键生成）：**

```bash
cp env/production.env.example env/production.env
# ① 替换 7 个 <CHANGE_ME>（1 个数据库密码 + 6 个 Cloud Router 密钥，§4.1 有生成命令；
#    本地验证可偷懒：bash prepare-envs.sh --dev-secrets-for-production）
# ② 外置模式：填 GATEWAY_POSTGRES_* 连接信息（sslmode=require 示例已预置；
#    完整的外置 PostgreSQL 与 Redis 配置步骤与示例见 §4.2）
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

**一切从打包开始**：拿到安装包才有后面的一切。全流程只用 shell 脚本，在仓库根目录
一条命令产出自包含安装包（镜像 + compose + env 模板 + 部署脚本）：

```bash
bash scripts/build-install-bundle.sh                      # 全量打包：编译 → 镜像 → bundle
bash scripts/build-install-bundle.sh --skip-server-package # Rust 没改过：跳过编译，复用 server 包
bash scripts/build-install-bundle.sh --skip-image-build    # 镜像没改过：跳过镜像构建，只重打 bundle
bash scripts/build-install-bundle.sh --dry-run            # bundle 步骤只看计划
```

脚本自动完成四步：cargo release 编译出服务包 → 构建 OCI 镜像 → 按
`sdkwork.app.config.json` 的 `release.currentVersion`（当前 0.1.1）打 registry tag →
打出 `dist/docker-install/sdkwork-api-cloud-gateway-install-<版本>.bundle/`。
构建机需要：Docker（含 compose 插件）、Node.js、Rust 工具链（cargo）——包装命令只有上面这一条 bash。

### 2.1 重新打包（日常迭代怎么选）

按「这次到底改了什么」选跳过项，能省大量时间：

| 这次改了什么 | 命令 | 耗时感受 |
| --- | --- | --- |
| Rust 代码 / Cargo 依赖 | `bash scripts/build-install-bundle.sh`（全量） | 最长（编译 + 镜像） |
| 只改了打包脚本 / compose / env 模板 / 文档 | `bash scripts/build-install-bundle.sh --skip-server-package` | 中（只重做镜像与 bundle） |
| 只改了 bundle 装配（deploy.sh / prepare-envs.sh / README） | `bash scripts/build-install-bundle.sh --skip-server-package --skip-image-build` | 最快（秒级重装 bundle） |
| 只想看会执行什么 | 加 `--dry-run` | 秒级 |

**版本号**：新版本发布前把 `sdkwork.app.config.json` 的 `release.currentVersion`
改为新版本号（如 `0.1.2`），或临时用 `--version 0.1.2` 覆盖——版本号决定镜像 tag
（`registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:<版本>`）与 bundle 目录名。
**每次发布的 bundle 目录独立存在、互不覆盖**，旧版本目录保留即可用于回滚（§5.2）。

**分发前自检**：新 bundle 的 `image.sha256` 与 `manifest.json` 记录了镜像指纹，
拷贝到远程后用 `sha256sum -c` 校验（§3）；`prepare-envs.sh` 不会覆盖已有 env，
重复打包不会弄丢服务器上的配置。

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

## 3. 第二步：拷贝到远程服务器

把整个 `.bundle` 目录拷到目标 Linux 主机。目标机要求只有两个：
**装好 Docker（含 compose 插件）**、**能访问镜像仓库或已随包携带 image.tar.gz**。

手动拷贝（scp / rsync 任选，注意结尾的 `/` 语义）：

```bash
# scp（最简单）
scp -r dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.1.bundle \
    deploy@api.sdkwork.com:/opt/sdkwork/deploy/

# rsync（断点续传，重跑只传差异；推荐大包使用）
rsync -avP dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.1.bundle \
    deploy@api.sdkwork.com:/opt/sdkwork/deploy/
```

拷完在远程主机上校验完整性（离线主机必做）：

```bash
ssh deploy@api.sdkwork.com 'cd /opt/sdkwork/deploy/sdkwork-api-cloud-gateway-install-0.1.1.bundle \
    && sha256sum -c image.sha256'   # 必须输出 image.tar.gz: OK
```

不想手动拷？§8 的远程一键部署脚本会自动完成「打包 → 推送 → 安装 → 健康检查」全链路。

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
在仓库内一条 shell 命令一键创建（**完整的外置 PostgreSQL + Redis 配置步骤与示例见 §4.2**）：

```bash
bash scripts/provision-external-databases.sh
```

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

### 4.2 外置 PostgreSQL 与 Redis：配置步骤（命令 + 示例）

外置模式（默认）要求你先准备好外部 PostgreSQL（**必须带 pgvector 扩展**）和 Redis，
再把连接信息填进 env。本节给出从零到连通的完整命令。

**命名约定**（数据库 = schema = 角色同名，脚本与 SQL 都遵循）：

| 环境 | 数据库 / Schema / 角色 | Redis 逻辑库（RTC / Web） |
| --- | --- | --- |
| development | `sdkwork_ai_dev` | 1 / 2 |
| test | `sdkwork_ai_test` | 3 / 4 |
| staging | `sdkwork_ai_staging` | 5 / 6 |
| demo | `sdkwork_ai_demo` | 9 / 10 |
| production | `sdkwork_ai_prod` | 7 / 8 |

**第 1 步：准备 PostgreSQL 服务器**（已有可跳过；必须是带 pgvector 的实例）：

```bash
# Ubuntu 安装 PostgreSQL + pgvector（版本号按实际，如 14/16/18）
sudo apt install postgresql postgresql-14-pgvector

# 或者用 Docker 起一个（数据落在具名卷，重启不丢）
docker run -d --name sdkwork-pg --restart unless-stopped \
  -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=<管理员密码> \
  -p 5432:5432 -v sdkwork-pg-data:/var/lib/postgresql/data \
  pgvector/pgvector:pg18
```

**第 2 步：建库 / 建 schema / 装扩展 / 建角色** —— 二选一：

```bash
# 方式 A（推荐）：一键脚本。在仓库根目录执行；先在各 env 文件里填好
# GATEWAY_POSTGRES_PASSWORD，脚本会用它创建同名角色。
export SDKWORK_DATABASE_ADMIN_HOST=<PG主机>
export SDKWORK_DATABASE_ADMIN_PORT=5432
export SDKWORK_DATABASE_ADMIN_USERNAME=postgres
export SDKWORK_DATABASE_ADMIN_PASSWORD='<管理员密码>'
bash scripts/provision-external-databases.sh               # 全部 5 个环境
bash scripts/provision-external-databases.sh production    # 或只供一个环境

# 方式 B：手工 SQL（DBA 审核场景）。数据库、schema、角色三个动作：
psql -h <PG主机> -U postgres -c "CREATE DATABASE sdkwork_ai_prod;"
# 在该库内建同名 schema + 在 schema 内安装 pgvector（需超级用户）
psql -h <PG主机> -U postgres -d sdkwork_ai_prod \
  -v db=sdkwork_ai_prod -v app_user=sdkwork_ai_prod \
  -f docker/postgres/external-schema.sql
# 建/改角色并授权
psql -h <PG主机> -U postgres -c \
  "CREATE ROLE sdkwork_ai_prod LOGIN PASSWORD '<GATEWAY_POSTGRES_PASSWORD>';"
psql -h <PG主机> -U postgres -d sdkwork_ai_prod -c \
  "GRANT ALL PRIVILEGES ON DATABASE sdkwork_ai_prod TO sdkwork_ai_prod;"
```

**第 3 步：准备 Redis**（已有可跳过）。**默认方案是无密码**（env 模板默认值
`GATEWAY_CLOUDROUTER_REDIS_PASSWORD=` 留空即对应免密）：

```bash
# 默认方案：无密码（内网 / 受信网络），仅 AOF 持久化
docker run -d --name sdkwork-redis --restart unless-stopped \
  -p 6380:6379 -v sdkwork-redis-data:/data \
  redis:8-alpine redis-server --appendonly yes

# 验证连通（返回 PONG）
redis-cli -h <REDIS主机> -p 6380 ping
```

可选加固（公网 / 跨信任域才需要）——带密码版本：

```bash
# 加密方案：requirepass + AOF 持久化
docker run -d --name sdkwork-redis --restart unless-stopped \
  -p 6380:6379 -v sdkwork-redis-data:/data \
  redis:8-alpine redis-server --requirepass '<Redis密码>' --appendonly yes

# 验证连通（返回 PONG）
redis-cli -h <REDIS主机> -p 6380 -a '<Redis密码>' ping
```

自建/托管 Redis 均可，只要网络可达（带密码方案还需密码已知）。同一 Redis
服务多环境共用时，靠 env 里的逻辑库编号隔离（见上表）；也可每环境独立
Redis 实例。

**第 4 步：把连接信息填进 `env/<环境>.env`**（以 production 为例）：

```bash
# --- PostgreSQL（对应第 2 步产物）---
GATEWAY_POSTGRES_HOST=<PG主机>            # 网关跑在容器里：宿主机用 host.docker.internal
GATEWAY_POSTGRES_PORT=5432
GATEWAY_POSTGRES_DB=sdkwork_ai_prod
GATEWAY_POSTGRES_SCHEMA=sdkwork_ai_prod
GATEWAY_POSTGRES_USER=sdkwork_ai_prod
GATEWAY_POSTGRES_PASSWORD=<第 2 步设置的角色密码>
GATEWAY_POSTGRES_SSL_MODE=require         # production/staging 强制；内网 dev/test 可 disable

# --- Redis 方案一（默认，无密码）：密码行留空，URL 不带密码段 ---
GATEWAY_CLOUDROUTER_REDIS_HOST=<REDIS主机>
GATEWAY_CLOUDROUTER_REDIS_PORT=6380
GATEWAY_CLOUDROUTER_REDIS_PASSWORD=
GATEWAY_RTC_STATE_REDIS_URL=redis://<REDIS主机>:6380/7
GATEWAY_WEB_REDIS_URL=redis://<REDIS主机>:6380/8

# --- Redis 方案二（可选加固，带密码）：三行同步填，URL 密码须百分号编码 ---
# GATEWAY_CLOUDROUTER_REDIS_PASSWORD=<Redis密码>
# GATEWAY_RTC_STATE_REDIS_URL=redis://:<URL编码后的Redis密码>@<REDIS主机>:6380/7
# GATEWAY_WEB_REDIS_URL=redis://:<URL编码后的Redis密码>@<REDIS主机>:6380/8
```

> 注意三点：
> ① **两套方案切换时要三处同步**：`GATEWAY_CLOUDROUTER_REDIS_PASSWORD`、
> `GATEWAY_RTC_STATE_REDIS_URL`、`GATEWAY_WEB_REDIS_URL` 的密码状态必须一致
> （免密 = 密码行留空 + URL 无密码段；带密 = 三处都填）。
> ② 带密方案里 URL 是完整 URL，密码中的 `@ : / # ?` 等字符必须百分号编码
> （python3 `urllib.parse.quote` 一行转换）；逻辑库编号按上表按环境区分。
> ③ env 模板默认 `host.docker.internal`（容器内访问宿主机服务）；Linux 宿主上
> compose 模板已自动加 `host-gateway` 映射，直接填外部主机名/IP 也可。

**第 5 步：部署前连通性验证**（两条都过再 `deploy.sh`）：

```bash
psql "host=<PG主机> port=5432 dbname=sdkwork_ai_prod user=sdkwork_ai_prod \
  password=<角色密码> sslmode=require" -c "SELECT extname FROM pg_extension WHERE extname='vector';"
# 预期输出一行 vector

redis-cli -h <REDIS主机> -p 6380 ping                     # 无密码方案，预期 PONG
redis-cli -h <REDIS主机> -p 6380 -a '<Redis密码>' ping    # 带密码方案，预期 PONG
```

## 5. 第四步：部署与验证

`deploy.sh` 是唯一入口，幂等可重复执行：

```bash
bash deploy.sh --environment development                  # 开发环境，单实例，外置依赖
bash deploy.sh --environment development --embedded       # 同上，内置 PostgreSQL + Redis
bash deploy.sh --environment production --replicas 3      # 生产 3 实例（实例 1 负责迁移）

bash deploy.sh --environment test --ps                    # 看状态
bash deploy.sh --environment test --logs 1                # 跟踪实例 1 日志
bash deploy.sh --environment test --restart               # 重启（deps → 实例依次重启并等健康）
bash deploy.sh --environment test --stop                  # 停止（保留容器与数据卷）
bash deploy.sh --environment test --start                 # 停止后的成套启动（含健康等待）
bash deploy.sh --environment test --down                  # 摘除（删除容器，保留数据卷）
bash deploy.sh --environment test --down --purge          # 摘除并删除卷/网络（内置库数据也会删除！）

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

### 5.1 Day-2 日常运维（重启 / 停止 / 日志 / 文件位置）

所有运维动作同样走 `deploy.sh`，在 bundle 目录执行（远程机为
`/opt/sdkwork/deploy/sdkwork-api-cloud-gateway`）：

| 动作 | 命令 | 说明 |
| --- | --- | --- |
| 看状态 | `bash deploy.sh --environment <env> --ps` | 实例 + deps 容器状态与健康检查 |
| 看日志 | `bash deploy.sh --environment <env> --logs 1` | 跟踪实例 1 日志（`--logs 2` 为实例 2，以此类推） |
| 重启 | `bash deploy.sh --environment <env> --restart` | deps → 实例依次重启，**等健康后才返回**（失败即退出非 0） |
| 停止 | `bash deploy.sh --environment <env> --stop` | 全部容器 stop，**保留容器与数据卷**，`--start` 秒级恢复 |
| 启动 | `bash deploy.sh --environment <env> --start` | 停止后的成套启动（deps → 实例，含健康等待）；首次部署请用 apply |
| 摘除 | `bash deploy.sh --environment <env> --down` | 删除容器/网络，**保留数据卷** |
| 连卷删除 | `bash deploy.sh --environment <env> --down --purge` | 数据卷一并删除（内置库数据也会删除！） |

> `--stop/--start/--restart` 与 `--down` 的区别：前者保留容器（`docker stop/start/restart` 语义，
> 启动快），后者删除容器后需重新 up；两者都保留数据卷（除非 `--purge`）。
> **改了 env 配置后 `--restart` 不会生效**（restart 只重启进程，不重建容器）——重跑一次
> apply（幂等）即可让新配置生效：`bash deploy.sh --environment <env> --replicas N`。

**远程服务器上做同样的事**（在本机执行，动作转发到远程 deploy.sh）：

```bash
bash scripts/docker/remote-deploy.sh --host <user@host> --environment production --restart
bash scripts/docker/remote-deploy.sh --host <user@host> --environment production --stop
bash scripts/docker/remote-deploy.sh --host <user@host> --environment production --ps
bash scripts/docker/remote-deploy.sh --host <user@host> --environment production --logs 1
```

**日志与状态的原生命令**（deploy.sh 之外，直接用 docker 等价可用）：

```bash
docker ps --filter name=sdkwork-api-cloud-gateway                   # 全部相关容器
docker logs -f --tail 200 sdkwork-api-cloud-gateway-<env>-i1-gateway-1              # 实例 1 网关日志
docker logs -f --tail 200 sdkwork-api-cloud-gateway-<env>-deps-knowledgebase-rpc-1  # deps 单例日志
docker compose -p sdkwork-api-cloud-gateway-<env>-i1 ps             # 单实例项目状态
```

**部署文件与配置位置**：

| 内容 | 本机（bundle 目录） | 远程 Ubuntu |
| --- | --- | --- |
| 部署脚本 | `<bundle>/deploy.sh` | `/opt/sdkwork/deploy/sdkwork-api-cloud-gateway/deploy.sh` |
| 环境配置（唯一需要编辑的文件） | `<bundle>/env/<环境>.env` | `/etc/sdkwork/api-gateway/docker/<环境>.env` |
| 单实例差异配置（可选） | `<bundle>/env/<环境>.i<序号>.env` | 同目录 `<环境>.i<序号>.env` |
| Compose 模板 | `<bundle>/compose/docker-compose.bundle.yml`（内置模式为 `docker-compose.bundle.embedded.yml`） | 同 bundle 目录 |
| 数据卷 | `<环境>-postgres-data` / `<环境>-redis-data`（内置模式） | 同名 |
| 容器项目名 | 实例 `sdkwork-api-cloud-gateway-<env>-i<N>`；依赖 `sdkwork-api-cloud-gateway-<env>-deps` | 同名 |

### 5.2 回滚流程（版本回退 / 配置回退 / 数据回退）

**升级前先做三件事，回滚才有退路**（每次升级都执行）：

```bash
# ① 备份当前 env 配置
cp /etc/sdkwork/api-gateway/docker/<环境>.env \
   /etc/sdkwork/api-gateway/docker/<环境>.env.bak.$(date +%F-%H%M)
# ② 保留旧版本 bundle（远程 rsync 安装新版本前，把旧目录拷一份带版本号）
cp -r /opt/sdkwork/deploy/sdkwork-api-cloud-gateway \
      /opt/sdkwork/deploy/sdkwork-api-cloud-gateway-v0.1.1
# ③ 数据库备份：外置模式按你自己的数据库流程备份；
#    内置模式用下面「数据回退」里的 pg_dump 命令
```

**① 应用版本回退（最常用）**：每个 bundle 自带对应版本的 `image.tar.gz`，
回滚 = 用旧 bundle 重新 apply（同一 compose 项目名，幂等重建容器）：

```bash
cd /opt/sdkwork/deploy/sdkwork-api-cloud-gateway-v0.1.1     # 上一版本的 bundle
bash deploy.sh --environment production \
  --env-file /etc/sdkwork/api-gateway/docker/production.env --replicas 3
curl --noproxy '*' http://127.0.0.1:3913/healthz            # 200 即回滚完成
```

如果新旧镜像都已经在本机（`docker images | grep sdkwork-api-cloud-gateway` 可见），
在**当前** bundle 里指定旧 tag 更快——镜像解析优先级：`--image-tag` > bundle `image.env`
> env 的 `GATEWAY_IMAGE` > 默认版本：

```bash
bash deploy.sh --environment production --image-tag 0.1.0 \
  --env-file /etc/sdkwork/api-gateway/docker/production.env --replicas 3
```

**② 配置回退**：恢复 env 备份后重跑 apply（幂等，配置即生效）：

```bash
cp /etc/sdkwork/api-gateway/docker/<环境>.env.bak.2026-09-03-1500 \
   /etc/sdkwork/api-gateway/docker/<环境>.env
bash deploy.sh --environment <环境> \
  --env-file /etc/sdkwork/api-gateway/docker/<环境>.env --replicas N
```

**③ 数据回退（仅破坏性迁移 / 数据损坏时）**：数据库迁移是**正向执行**的——
应用版本回退不会回滚 schema（旧版本兼容新 schema），所以一般回滚只做①。
只有新版本含破坏性迁移或数据被写坏时才恢复数据库：

```bash
# 备份（内置模式；外置模式在数据库侧做，网关无需变更）
docker exec sdkwork-api-cloud-gateway-<env>-deps-postgres-1 \
  pg_dump -U <库名同名用户> -d <数据库名> -Fc > pg-<env>-$(date +%F).dump

# 恢复：先停后恢复再启动
bash deploy.sh --environment <env> --stop
cat pg-<env>-2026-09-03.dump | docker exec -i \
  sdkwork-api-cloud-gateway-<env>-deps-postgres-1 \
  pg_restore -U <库名同名用户> -d <数据库名> --clean --if-exists
bash deploy.sh --environment <env> --start
```

**④ 回滚后验证清单**：

```bash
bash deploy.sh --environment <env> --ps                    # 实例与 deps 全部 healthy
curl --noproxy '*' http://127.0.0.1:<端口>/healthz         # 200
curl --noproxy '*' http://127.0.0.1:<端口>/readyz          # 200
docker logs --tail 100 sdkwork-api-cloud-gateway-<env>-i1-gateway-1   # 无 ERROR / 崩溃循环
```

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

## 8. 远程一键部署（打包 → 推送 → 安装 → 健康检查）

`scripts/docker/remote-deploy.sh` 把 §2–§5 串成一条命令：本地打包（或复用已有
bundle）→ SSH 推送到远程主机 → 物化远程配置 → 远程执行 deploy.sh → 健康检查。
配置以远程为准、重复执行不覆盖：

```bash
# 前置：ssh-copy-id deploy@api.sdkwork.com 一次（免密登录）
bash scripts/docker/remote-deploy.sh --host deploy@api.sdkwork.com \
    --environment production --replicas 3

# 内置依赖的远程部署（远程机无需自备 PostgreSQL/Redis）
bash scripts/docker/remote-deploy.sh --host deploy@api.sdkwork.com \
    --environment development --embedded

# 常用：复用已有 bundle / 指定密钥 / 预演（只打印将执行的命令）
bash scripts/docker/remote-deploy.sh --host deploy@api.sdkwork.com \
    --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.1.bundle \
    --ssh-key ~/.ssh/prod_ed25519 --environment test --replicas 2 --dry-run
```

远程配置文件：`/etc/sdkwork/api-gateway/docker/<环境>.env`（权限 0600，首推自动生成）。
改密码 / 改数据库 → 编辑该文件 → 重新执行同命令即生效。需要远程 sudo 免密（`sudo -n`），
否则用 `--config-root` 指向用户可写目录。Day-2 操作同样直连远程：`--ps` / `--logs N` /
`--restart` / `--stop` / `--start` / `--down` / `--down --purge` 会转发给远程的 deploy.sh（详见 §5.1）。

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

**升级 / 回滚准备**（每次升级前过一遍，详见 §5.2）：

- [ ] env 配置已备份（`<环境>.env.bak.<时间戳>`）
- [ ] 上一版本 bundle 已保留（带版本号目录），`image.tar.gz` 完整
- [ ] 数据库已有当前时点备份（外置按自有流程；内置用 §5.2 pg_dump）
- [ ] 回滚演练过一次 `--image-tag <旧版本>` 或旧 bundle apply

完整操作手册：`docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md`；
Ubuntu 裸机（systemd）安装见 `docs/guides/operator/INSTALLATION_STANDARDS.md`。

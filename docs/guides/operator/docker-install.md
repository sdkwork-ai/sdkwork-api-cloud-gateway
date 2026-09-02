# Docker 傻瓜式安装指南（docker-install）

> 版本：2026-09-02 · 适用：`sdkwork-api-cloud-gateway` standalone 统一安装镜像
> 规范依据：`sdkwork-specs/DEPLOYMENT_SPEC.md` §6、`sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4
> English version: [docker-install.en.md](./docker-install.en.md)

## 0. 三条命令跑起来（TL;DR）

拿到安装包（`.bundle` 目录）后，在任意 Docker 主机上：

```bash
cp env/development.env.example env/development.env   # 1. 生成配置（默认值即可先冒烟）
bash deploy.sh --environment development             # 2. 部署（默认外置数据库）
curl http://127.0.0.1:3910/healthz                   # 3. 健康检查，返回 ok 即成功
```

没有现成 PostgreSQL / Redis？加一个 `--embedded`，内置数据库随包启动：

```bash
bash deploy.sh --environment development --embedded
```

就这么多。其余章节是细节：什么时候选哪种模式、每个环境怎么配、怎么扩容和排障。

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

## 2. 第一步：打包安装包（构建机）

在仓库根目录一条命令产出自包含安装包（镜像 + compose + env 模板 + 部署脚本）：

```bash
pnpm build:container:install                        # 构建镜像 + 打包
pnpm build:container:install -- --skip-image-build  # 复用已构建镜像，只重新打包
pnpm build:container:install -- --dry-run           # 只看计划，不实际执行
```

产物布局：

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
├── manifest.json                             # 版本 / 镜像 / sha256 元数据
└── README.md
```

## 3. 第二步：拷贝到目标机

把整个 `.bundle` 目录拷贝到目标 Linux 主机（scp / rsync / U 盘均可），要求只有两个：
**装好 Docker（含 compose 插件）**、**能访问镜像仓库或已随包携带 image.tar.gz**。

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
| `GATEWAY_POSTGRES_SSL_MODE` | 生产/预发建议 `require`（示例已预置） | 建议 | 自动启用 TLS |
| `GATEWAY_DEPS_MODE` | `external`（默认）或 `embedded` | 不用动 | 可写 `embedded` 代替 `--embedded` |
| `GATEWAY_CLOUDROUTER_API_KEY_PEPPER` 等 6 个密钥 | 生产环境 `<CHANGE_ME>` 占位必须替换 | 生产必填 | 生产必填 |

**密码包含 URL 特殊字符时**：不要直接填 `GATEWAY_POSTGRES_PASSWORD`，改用百分号编码后的完整
`SDKWORK_DATABASE_URL`（示例文件里有说明注释）。

外置模式的数据库与 schema 需按环境预先建好（`sdkwork_ai_dev/test/staging/demo/prod`），
仓库内可执行 `pnpm db:postgres:provision` 一键创建。

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

**验证**：

```bash
curl http://127.0.0.1:<实例端口>/healthz    # 存活
curl http://127.0.0.1:<实例端口>/readyz     # 就绪（首次启动需等迁移完成，最长约 4 分钟）
```

## 6. 多环境矩阵（端口 / 数据库规划）

实例端口公式：`实例 i 端口 = GATEWAY_HOST_PORT + (i-1) × 10 → 容器 3900`。
同一台机器可同时跑多个环境，端口与数据库天然隔离：

| 环境 | 基础端口（实例 1） | 数据库 / Schema | Redis 逻辑库 | 示例端口（3 实例） |
| --- | --- | --- | --- | --- |
| development | 3910 | `sdkwork_ai_dev` | 1 / 2 | 3910 / 3920 / 3930 |
| test | 3911 | `sdkwork_ai_test` | 3 / 4 | 3911 / 3921 / 3931 |
| staging | 3912 | `sdkwork_ai_staging` | 5 / 6 | 3912 / 3922 / 3932 |
| production | 3913 | `sdkwork_ai_prod` | 7 / 8 | 3913 / 3923 / 3933 |
| demo | 3914 | `sdkwork_ai_demo` | 9 / 10 | 3914 / 3924 / 3934 |

每个环境的配置集中在一份 env 文件（所有实例共用）：bundle 内 `env/<环境>.env`，或
远程 Ubuntu 的 `/etc/sdkwork/api-gateway/docker/<环境>.env`。需要单实例差异化时，
创建 `env/<环境>.i<序号>.env` 会自动叠加。

公网反向代理由独立的 `sdkwork-webserver` 部署持有：把各环境域名上游指向该环境的实例端口列表
即可（如生产 `api.{sdkwork,birdcoder,dtupay}.com` → `127.0.0.1:3913; 3923; 3933`）。
网关侧域名 allowlist / CORS 已在 env 示例中按环境预置。

## 7. 内置模式细节

- **启动内容**：deps 项目里多两个容器——`postgres`（pgvector 镜像，`ssl=on`，证书自动生成，
  schema 自动创建）与 `redis`（无密码）。网关与 knowledgebase-rpc 自动指向它们，无需改任何
  `GATEWAY_POSTGRES_*` / `GATEWAY_CLOUDROUTER_REDIS_*`。
- **数据持久化**：数据在 Docker 卷 `<环境>-postgres-data` / `<环境>-redis-data`，
  `--down` 保留、`--down --purge` 删除。备份：`docker run --rm -v <环境>-postgres-data:/data -v $PWD:/backup alpine tar czf /backup/pg.tar.gz /data`。
- **迁回外置**：先用 pg_dump 从内置库导出，再在外部 PostgreSQL 导入，最后去掉
  `--embedded` 重新执行 `deploy.sh` 并把 `GATEWAY_POSTGRES_*` 指向新实例。
- **限制**：内置库是单实例（无主从/集群），不承担生产级容量与高可用；生产环境请用外置。
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
    --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.0.bundle \
    --ssh-key ~/.ssh/prod_ed25519 --environment test --replicas 2 --dry-run
```

远程配置文件：`/etc/sdkwork/api-gateway/docker/<环境>.env`（权限 0600，首推自动生成）。
改密码 / 改数据库 → 编辑该文件 → 重新执行同命令即生效。需要远程 sudo 免密（`sudo -n`），
否则用 `--config-root` 指向用户可写目录。

## 9. 常见问题（FAQ）

**Q：`curl /healthz` 不通？**
按顺序查：`bash deploy.sh --environment <env> --ps` 看容器是否 healthy →
`--logs 1` 看启动日志 → 外置模式先确认 `GATEWAY_POSTGRES_HOST` 从容器内可达
（宿主机上跑的库用 `host.docker.internal`，env 示例默认值即是）→ 首次启动迁移耗时较长，
`/readyz` 最长等 4 分钟。

**Q：数据库密码怎么改？**
改 env 文件里的 `GATEWAY_POSTGRES_PASSWORD`（或整条 `SDKWORK_DATABASE_URL`）→
重新执行 `deploy.sh --environment <env>`。注意数据库侧的密码要同步修改。

**Q：怎么扩容 / 缩容？**
提高或降低 `--replicas N` 重新执行即可（幂等）。新实例自动跳过迁移；缩容只是停掉多余实例项目。

**Q：`--down --purge` 删了什么？**
环境的网络 + secrets/data 卷；内置模式下还会删除内置库数据卷（**不可恢复**）。外置模式不碰你的数据库。

**Q：多个环境能装在同一台机器上吗？**
可以。端口、网络、卷、数据库名全部按环境隔离，参考 §6 矩阵表。

**Q：镜像没有仓库可推？**
`image.tar.gz` 就是镜像本体，deploy.sh 检测到镜像缺失会自动 `docker load`，完全离线可用。

## 10. 运维检查清单

- [ ] `manifest.json` 中 image sha256 与 `image.sha256` 一致
- [ ] 每个环境 `/healthz` 与 `/readyz` 通过
- [ ] 多实例部署下仅实例 1 出现迁移日志
- [ ] 生产 / 预发 env 中所有 `<CHANGE_ME>` 占位已替换（含 6 个 Cloud Router 密钥）
- [ ] 生产 / 预发 `GATEWAY_POSTGRES_SSL_MODE=require`（外置模式）
- [ ] sdkwork-webserver 各环境域名经反代可达网关 API

完整操作手册：`docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md`；
Ubuntu 裸机（systemd）安装见 `docs/guides/operator/INSTALLATION_STANDARDS.md`。

# Docker 安装包部署指南（docker-install）

> 版本：2026-08-30 · 适用：`sdkwork-api-cloud-gateway` standalone 统一安装镜像
> 规范依据：`sdkwork-specs/DEPLOYMENT_SPEC.md` §6、`sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4

## 1. 设计概要

`pnpm build:container:install` 产出**一个统一安装镜像包**（self-contained install bundle）：

- **一个镜像**：镜像本身与环境和实例数无关（environment-neutral），构建时**不**烘焙任何环境、域名、数据库或凭据信息；生命周期环境（development / test / staging / demo / production）与实例数全部是**部署时输入**，由容器 entrypoint 启动时解析。
- **任意环境**：development、test、staging、demo、production 使用同一个镜像 tag，部署时通过 env 文件选择环境（`GATEWAY_ENVIRONMENT`）。demo（独立演示层）与 dev/test/staging/prod 完全隔离（独立数据库 `sdkwork_ai_demo`、独立 Redis 逻辑库 9/10）。
- **每个环境支持多实例**：N 个网关实例共享同一网络（`sdkwork-api-cloud-gateway-<env>`）与同一份 secrets/data 卷；每个实例拥有独立 compose 项目名（`sdkwork-api-cloud-gateway-<env>-i<index>`）和独立宿主机端口；实例 1 先执行数据库迁移，实例 2..N 在其健康后才启动。
- **依赖外部化**：bundle 不内置 PostgreSQL/Redis；`GATEWAY_POSTGRES_*` / `GATEWAY_CLOUDROUTER_REDIS_*` 指向外部实例。公网反向代理由独立的 `sdkwork-webserver` 部署持有，本 bundle 不安装 nginx。

## 2. 打包安装包

```bash
# 仓库根目录（sdkwork-api-cloud-gateway）
pnpm build:container:install                       # 构建镜像 + 打包
pnpm build:container:install -- --skip-image-build # 复用已构建镜像
pnpm build:container:install -- --tag 0.1.0 --out dist/docker-install --dry-run
```

从 sdkwork-space 根目录（WSL / CI）也可以走 `bin` 脚本：

```bash
bash bin/build-gateway-docker.sh                   # 默认读取 manifest 版本
bash bin/build-gateway-docker.sh --out /opt/deploy/packages
```

产物布局：

```text
dist/docker-install/sdkwork-api-cloud-gateway-install-<version>.bundle/
├── image.tar.gz / image.sha256 / image.env   # 镜像归档 + 校验 + tag
├── compose/
│   └── docker-compose.bundle.yml             # 环境中立的多实例模板
├── env/
│   ├── development.env.example
│   ├── test.env.example
│   ├── staging.env.example
│   ├── demo.env.example
│   └── production.env.example
├── deploy.sh                                 # 通用部署脚本（唯一入口）
├── manifest.json                             # 版本 / 镜像 / sha256 元数据
└── README.md
```

## 3. 部署（任意 Docker 主机）

前置条件：外部 PostgreSQL（建议 pgvector 扩展）与 Redis 可达（`host.docker.internal` 或托管端点），数据库与 schema 已按环境创建（`sdkwork_ai_dev/test/staging/demo/prod`，可先用 `pnpm db:postgres:provision`）。

把 bundle 目录拷贝到目标主机后，唯一入口是 `deploy.sh`：

```bash
# 开发环境单实例（默认）
bash deploy.sh --environment development

# 生产环境 3 实例
bash deploy.sh --environment production --replicas 3

# 其它操作
bash deploy.sh --environment test --ps
bash deploy.sh --environment test --logs 2        # 跟踪实例 2 日志
bash deploy.sh --environment test --down
bash deploy.sh --environment test --down --purge  # 连同卷/网络一并删除
```

关键规则：

- `--environment` 必填（development | test | staging | demo | production），缺失或未知**在产生任何副作用之前**报错退出；重复执行 apply 幂等（原地更新既有栈）。
- 首次部署会从 `env/<environment>.env.example` 生成 `env/<environment>.env`，**请先填好 PostgreSQL、Redis、Cloud Router 密钥再对外暴露**。
- 若镜像未加载，脚本自动 `docker load image.tar.gz`。
- 启动顺序：每环境一个 `knowledgebase-rpc` 单例（deps 项目，负责预置共享 mTLS/密钥材料）→ 网关实例 1（执行 `--migrate-databases`）→ 实例 2..N（`GATEWAY_MIGRATE_ON_START=false`，避免迁移竞态）。

仓库内等价命令：

```bash
pnpm deploy:apply:standalone:docker -- --environment production --replicas 3
pnpm deploy:apply:standalone:docker -- --environment development --down
```

## 4. 端口计划与 sdkwork-webserver 对接

实例端口公式（确定性的、跨环境无冲突）：

```text
实例 i 宿主机端口 = GATEWAY_HOST_PORT + (i - 1) * 10   ->  容器 3900
```

| 环境 | 基础端口（实例 1） | 数据库 / Schema | 实例端口示例（3 实例） |
| --- | --- | --- | --- |
| development | 3910 | `sdkwork_ai_dev` | 3910 / 3920 / 3930 |
| test | 3911 | `sdkwork_ai_test` | 3911 / 3921 / 3931 |
| staging | 3912 | `sdkwork_ai_staging` | 3912 / 3922 / 3932 |
| production | 3913 | `sdkwork_ai_prod` | 3913 / 3923 / 3933 |
| demo | 3914 | `sdkwork_ai_demo` | 3914 / 3924 / 3934 |

健康检查：`curl http://127.0.0.1:<实例端口>/healthz`（就绪探针 `/readyz`）。

### 4.1 直接通过宿主机 IP 访问

容器内网关绑定 `0.0.0.0:3900`，实例端口发布在宿主机所有接口上，因此
`http://<宿主机IP>:<实例端口>/healthz` 天然可达。Host 头校验（安全层）按
环境有两种姿态：

- development / test / demo：env 预置 `GATEWAY_SECURITY_ALLOW_IP_HOSTS=true`，
  网关接受任意 IP 字面量 Host（`security.allowIpHosts`），任何机器直接
  `curl http://<主机IP>:3910/healthz` 即可，无需为每台机器改 allowlist。
- staging / production：保持精确域名 allowlist（`api…` 域名族）。如需 IP
  直连（健康探针、运维 smoke check），二选一：把该 IP 追加进
  `GATEWAY_ALLOWED_HOSTS`，或显式设置 `GATEWAY_SECURITY_ALLOW_IP_HOSTS=true`。

`sdkwork-webserver` 是独立的 Docker 部署，负责公网反代：把每个环境域名的
`/app/`、`/backend/` 等 API 上游指向该环境的网关实例端口列表做负载均衡
（例如 production 域名 `api.{sdkwork,birdcoder,dtupay}.com` →
`127.0.0.1:3913; 127.0.0.1:3923; 127.0.0.1:3933`）。网关侧的
`GATEWAY_ALLOWED_HOSTS` / `GATEWAY_CORS_ALLOWED_ORIGINS` 需包含对应域名
（env 文件已按环境预置）。

## 5. 多实例注意事项

- **迁移归属**：只有实例 1 执行迁移（entrypoint 的 `--migrate-databases` 生命周期步骤）；扩容（提高 `--replicas` 重跑 apply）新实例自动跳过迁移。
- **共享密钥**：secrets 卷按环境共享。首启实例以 noclobber 方式生成支付主密钥 / IAM 签名主密钥 / Knowledgebase RPC mTLS 材料；多实例并发首启安全（仅一个进程创建）。合规要求更严格时，把 env 文件中 `GATEWAY_PROVISION_*` 置为 `false` 并挂载运营方/KMS 管理的密钥。
- **无状态水平扩展**：网关实例共享 PostgreSQL 与 Redis（RTC 状态、web 会话走 Redis DB index 隔离），可随时 `--replicas N` 重建或缩容。
- **实例级覆盖**：需要单实例差异化配置时，创建 `env/<environment>.i<index>.env`，deploy.sh 会叠加在基础 env 之上（后者 `--env-file` 优先）。
- **唯一节点身份**：deploy.sh 按实例推导 `SDKWORK_IM_ID_NODE_ID`（实例 1 用 env 基值，实例 2..N 依次 +1），保证同环境多实例的雪花 ID 生成不冲突（DEPLOYMENT_SPEC §6）。
- **资源限制**：每实例默认 2 CPU / 2GiB 上限，通过 `GATEWAY_CPU_LIMIT` / `GATEWAY_MEMORY_LIMIT` 调整。

## 6. 运维与验证清单

```bash
bash deploy.sh --environment <env> --ps        # 各实例与 knowledgebase-rpc 状态
bash deploy.sh --environment <env> --logs 1    # 跟踪实例日志
bash deploy.sh --environment <env> --down      # 停止（保留卷/网络）
```

- [ ] `docker compose -f compose/docker-compose.bundle.yml config` 无插值错误
- [ ] 每个环境 `curl /healthz` 与 `/readyz` 通过
- [ ] 多实例部署下仅实例 1 出现迁移日志
- [ ] sdkwork-webserver 各环境域名经反代可达网关 API
- [ ] `manifest.json` 中 image sha256 与 `image.sha256` 一致

完整操作手册：`docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md`。

## 7. 远程 Ubuntu 部署（SSH 免密推送）

本地打包完成后，通过 **SSH 密钥免密登录** 将 bundle 推送至远程 Ubuntu 主机并
就地部署（数据库配置集中在远程 Ubuntu 的规范配置目录中，所有实例共用）。

```bash
# 仓库根目录
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --environment production --replicas 3

# 复用已有 bundle / 指定密钥
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.0.bundle \
    --ssh-key ~/.ssh/prod_ed25519 --environment test --replicas 2

# 其它操作（透传远程动作）
pnpm deploy:apply:standalone:docker:remote -- --host deploy@1.2.3.4 \
    --environment test --down
```

从 sdkwork-space 根目录（WSL / CI）：

```bash
bash bin/remote-deploy-gateway.sh --host deploy@1.2.3.4 \
    --environment production --replicas 3
```

### 7.1 前置条件

- **SSH 免密**：一次性执行 `ssh-copy-id deploy@1.2.3.4`（默认使用
  `~/.ssh/id_ed25519`，可用 `--ssh-key` 指定）。
- **远程 sudo**：默认配置目录在 `/etc/sdkwork/api-gateway/docker`（规范配置根，
  `APPLICATION_DEPLOY_LAYOUT_SPEC` §3），远程用户需要免密 sudo（`sudo -n`）；
  否则用 `--config-root` 指向用户可写目录。

### 7.2 配置目录与自定义数据库密码

推送后，环境配置写入远程 Ubuntu：

```text
/etc/sdkwork/api-gateway/docker/<environment>.env   # 唯一配置文件，所有实例共用
```

- 该文件**以远程为准**：重复部署不会覆盖（除非加 `--sync-env` 用本地文件强制覆盖）。
- **自定义数据库密码**：在远程主机编辑该文件中的 `GATEWAY_POSTGRES_PASSWORD`、
  `GATEWAY_POSTGRES_USER`、`GATEWAY_POSTGRES_DB` 等，然后重新执行
  `deploy:apply:standalone:docker:remote --environment <env>` 即生效；同一环境的
  N 个实例自动读取同一份配置。
- 首推若远程文件不存在，自动从本地 `docker/env/<env>.env`（无则 `.env.example`）
  推送生成；权限 `0600`。

### 7.3 执行流程

1. SSH 免密探测（`BatchMode=yes`，失败即提示 `ssh-copy-id`）；
2. 本地打包（可 `--bundle` 复用已有安装包）；
3. 远程创建 `install root` 与 `config root` 目录；
4. 推送/保留远程 env 配置；
5. `rsync -az`（无 rsync 时退化为 `scp -r`）推送 bundle；
6. 远程执行 `deploy.sh --environment <env> --replicas N --env-file <config-root>/<env>.env`；
7. 健康检查实例 1 的 `/healthz`。

支持 `--dry-run` 预演全部命令；`--logs [N]` / `--ps` / `--down` 透传远程动作。

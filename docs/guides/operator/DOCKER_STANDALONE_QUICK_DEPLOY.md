# Docker Standalone 快速部署（WSL Ubuntu + Windows）

本指南描述 `sdkwork-api-cloud-gateway` standalone 容器镜像的构建与 WSL 快速部署，
参考 `sdkwork-cloudrouter` 的容器组合模式。默认 development 部署通过三个域名访问网关：

- `http://api-dev.sdkwork.com`
- `http://api-dev.birdcoder.com`
- `http://api-dev.dtupay.com`

## 端口规划

| 端口 | 用途 |
| --- | --- |
| 3900 | `pnpm dev` 网关（Windows 本机） |
| 3901 | vite dev server（Windows 本机） |
| 3902 | 预留 client-only 入口 |
| 3903 | sdkwork-cloudrouter 容器（WSL） |
| **3910** | 本网关容器主机端口 → 容器内 3900（WSL，391x 保留段） |
| 80 | WSL nginx（三个测试域名的 HTTP 入口） |

## 环境要求（WSL Ubuntu）

- Docker Engine（daemon 运行中）：`docker version`
- nginx：`nginx -v`
- Node.js 22+ / pnpm 10（与 Windows 侧一致）
- Rust stable + cargo（构建 release 二进制）

## 1. 构建 release 二进制（WSL 内）

```bash
cd /mnt/e/sdkwork-space/sdkwork-api-cloud-gateway
export PYTHON=python3   # sdkwork-cloudrouter-http build.rs 需要 python 命令
CARGO_TARGET_DIR=/home/sdkwork/api-gateway-release-target \
  cargo build --release -p sdkwork-api-cloud-gateway \
  --bin sdkwork-api-cloud-gateway \
  --no-default-features --features platform-foundation-suite
```

`CARGO_TARGET_DIR` 指向 WSL ext4 磁盘（如 `/home/sdkwork/...`）可显著加速构建；
`PYTHON=python3` 是 Debian/Ubuntu 无 `python` 命令时的必需项。

## 2. 构建容器镜像

```bash
pnpm build:container          # 组装 dist/container-image-build 并 docker build
pnpm build:container:check    # 只校验构建计划（--check --dry-run）
```

产物：

- 镜像：`sdkwork-api-cloud-gateway:local`
- 构建上下文：`dist/container-image-build/`（含 bin/、config/、specs/、
  database-modules/、sdkwork.app.config.json、container/entrypoint、
  install-manifest.json、checksums.sha256）
- 镜像清单：`dist/container-image.json`（版本、imageId、远端 repoDigest、大小）。
  本地构建的 `repoDigest: null` 只能用于开发；发布必须通过工作流 `--push`，并在
  registry 回读 `repository@sha256` 成功后才生成 OCI artifact evidence。

发布工作流还会生成 `dist/container-image.cdx.json`、
`dist/container-image.intoto.jsonl`、`dist/container-image.sig`、公钥和
`.sdkwork/evidence/<package-id>.json`。部署必须按 evidence 中的完整 digest 拉取：

```bash
docker pull registry.sdkwork.com/sdkwork-api-cloud-gateway@sha256:<64-hex-digest>
docker image inspect --format '{{index .RepoDigests 0}}' \
  registry.sdkwork.com/sdkwork-api-cloud-gateway@sha256:<64-hex-digest>
```

不得用 `:latest`、版本 tag、本地 image ID 或 `container-image.json` 自身的文件哈希
替代 evidence 中的远端 OCI digest。CI 的 registry 用户名/令牌只通过
`SDKWORK_CONTAINER_REGISTRY_USERNAME` 和 `SDKWORK_CONTAINER_REGISTRY_PASSWORD`
secret 注入。

## 3. 启动容器组合

```bash
cp docker/env/development.env.example .env
# 替换所有 <CHANGE_ME> 后先执行部署预检
pnpm check:container-deployment -- --env-file .env --mode embedded --compose
docker compose --env-file .env up -d
docker compose --env-file .env ps
```

单机 Compose 默认设置 `GATEWAY_PROVISION_PAYMENT_CREDENTIAL_KEY=true`，首次启动时在
`gateway-secrets` 卷的 `/run/secrets/sdkwork/api_gateway/` 下生成权限为 `0600` 的
Payment 凭据主密钥，后续启动复用且不会覆盖。多副本部署必须将该值设为 `false`，并在
同一路径挂载由运维或 KMS 管理的共享密钥；丢失或让副本使用不同密钥会导致已有支付凭据
无法解密。

内嵌 PostgreSQL 首次启动会在其持久卷生成自签名服务证书并启用 TLS；staging 与
production 模板固定使用 `GATEWAY_POSTGRES_SSL_MODE=require`，避免生产态 IM 模块通过
明文数据库连接启动。只包含公开证书的 trust bundle 通过 `postgres-tls` 卷只读挂载到
网关，数据库私钥始终留在 `postgres-data`。外部 PostgreSQL 由运维提供证书和 TLS 策略。

检查就绪：

```bash
curl http://127.0.0.1:3910/healthz
curl http://127.0.0.1:3910/readyz
```

### 3.1 外连已有 / Ubuntu 宿主原生 PostgreSQL、Redis（不启动内置依赖）

数据库和 Redis 已存在（托管实例、既有安装，或 Ubuntu 宿主机 apt 安装的
`postgresql`/`redis-server`）时，使用 external override 启动，**不会创建**
compose 内置的 postgres/redis 容器：

```bash
cp docker/.env.example .env
# 在 .env 配置外连连接（示例见 .env.example「External services mode」与
# 「Ubuntu host-native services mode」）：
#   远端: GATEWAY_POSTGRES_HOST=db.internal.example.com
#   宿主: GATEWAY_POSTGRES_HOST=host.docker.internal   # override 已注入 host-gateway
#   GATEWAY_POSTGRES_PORT=5432
#   GATEWAY_POSTGRES_DB=sdkwork_ai_prod
#   GATEWAY_POSTGRES_SCHEMA=sdkwork_ai_prod           # 必须等于库名
#   GATEWAY_POSTGRES_USER=sdkwork_ai_prod
#   GATEWAY_POSTGRES_PASSWORD=...
#   GATEWAY_CLOUDROUTER_REDIS_HOST=...                # 远端 IP 或 host.docker.internal
#   GATEWAY_CLOUDROUTER_REDIS_PORT=6379
#   GATEWAY_CLOUDROUTER_REDIS_PASSWORD=             # 内网免密部署可留空
#   GATEWAY_CLOUDROUTER_REDIS_TLS=false
docker compose -f docker-compose.yml -f docker-compose.external.yml up -d
```

外连要求：

- 已有 PostgreSQL 中先建好规范化库与同名 schema（`docker-compose.external.yml`
  的内置依赖已被禁用，不会自动建库）。脚本已参数化，每个环境执行一次
  （`-v db` 同时决定库名与 schema 名，并创建 `vector` 扩展——宿主 PostgreSQL
  需先安装 `postgresql-18-pgvector` 等匹配大版本的包）：

  ```bash
  psql -h <host> -U <admin> -c "CREATE DATABASE sdkwork_ai_prod;"
  psql -h <host> -U <admin> -d sdkwork_ai_prod -v db=sdkwork_ai_prod \
    -v app_user=sdkwork_ai_prod -f docker/postgres/external-schema.sql
  ```

- 宿主原生模式（Ubuntu 自带服务）：PostgreSQL 需 `listen_addresses = '*'`
  并在 `pg_hba.conf` 放行 docker 网段（172.16.0.0/12），Redis 需
  `bind 0.0.0.0`（默认仅 127.0.0.1 无法被容器经 host-gateway 访问），
  详细步骤见 `docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md`
  「依赖部署三模式」章节。

- 宿主机上运行的外部服务用 `GATEWAY_POSTGRES_HOST=host.docker.internal` /
  `GATEWAY_CLOUDROUTER_REDIS_HOST=host.docker.internal` 可达（override 已注入
  host-gateway extra host）。
- 全新外部库只需预先创建数据库、同名 schema 和 `vector` 扩展。容器入口会先
  执行单进程 `--migrate-databases`，完整迁移 Models、CloudRouter 及其余嵌入模块，
  再以运行时自动迁移关闭的方式启动服务。由独立发布任务负责迁移时设置
  `GATEWAY_MIGRATE_ON_START=false`，并确保只有一个 elected migrator。
- 内置模式（默认 `docker compose up -d`）与外连模式互斥：外连时内置
  postgres/redis 不启动，网关直接连外部实例。

## 4. 配置 nginx（WSL）

```bash
sudo cp docker/nginx/api-dev.sdkwork.com.conf /etc/nginx/sites-enabled/sdkwork/
sudo nginx -t
sudo systemctl reload nginx
```

## 5. 绑定 Windows hosts

在 Windows 的 `C:\Windows\System32\drivers\etc\hosts` 追加（WSL 中可直接编辑）：

```
127.0.0.1 api-dev.sdkwork.com api-dev.birdcoder.com api-dev.dtupay.com
```

## 6. 验证

```bash
curl http://api-dev.sdkwork.com/healthz
curl http://api-dev.birdcoder.com/readyz
curl http://api-dev.dtupay.com/healthz
```

浏览器访问 `http://api-dev.sdkwork.com/`（网关 API 面，如 `/app/v3/api/iam/...`）。

## 配置说明

容器配置遵循 RUNTIME_DIRECTORY_SPEC §4.5 容器范围规范：

- **配置格式**：TOML（ENVIRONMENT_SPEC §2/§9："TOML is preferred for SDKWork Rust
  services"）。配置优先级：内置默认 → TOML → 环境变量 → CLI → 密钥
  （ENVIRONMENT_SPEC §3）。
- **配置位置**：`/etc/sdkwork/api_gateway/sdkwork-api-cloud-gateway.toml`（§4.5
  config mount）。镜像内置 production 模板为该路径的默认内容；`docker-compose.yml`
  将宿主文件 `docker/config/sdkwork-api-cloud-gateway.toml` 只读挂载覆盖之，
  修改限流/请求体/策略等环境变量未覆盖的字段无需重建镜像。
- **密钥**：`/run/secrets/sdkwork/api_gateway/<secret-name>`（§4.5 secret mount、
  §11 Secrets Standard）。本仓库不放置真实密钥；部署时把平台 secret 挂载到该目录
  并从配置或 `SDKWORK_*_FILE` 环境变量引用。禁止把密钥写进镜像层或 compose 文件。
- 部署期配置通过 `docker-compose.yml` / `.env` 注入（环境变量优先于 TOML）：
  - `SDKWORK_API_CLOUD_GATEWAY_ALLOWED_HOSTS` — 网关 Host 白名单（421 拒绝未列入域名）
  - `SDKWORK_CORS_ALLOWED_ORIGINS` — 浏览器 CORS 白名单（规范共享键）
  - `SDKWORK_DATABASE_*` — PostgreSQL 连接（compose 内置 postgres 服务）
  - `GATEWAY_HOST_PORT` — 主机映射端口（默认 3910）
- 数据卷：`postgres-data`、`gateway-data`（`/var/lib/sdkwork/api_gateway`，
  RUNTIME_DIRECTORY_SPEC §4.5 容器范围）。
- 多操作系统（Ubuntu systemd / Windows SCM / macOS launchd / 容器）的安装与配置
  规范见 [INSTALLATION_STANDARDS.md](INSTALLATION_STANDARDS.md)；Ubuntu systemd
  服务单元见 `deployments/systemd/`。

## 生产域名部署（api.sdkwork.com / api.birdcoder.com / api.dtupay.com）

同一网关容器可承载三个生产域名（nginx 按 `server_name` 分流，全部代理到
`127.0.0.1:3913`）：

1. **DNS**：三个域名解析到本机公网 IP（与测试域名不同，生产走真实 DNS，
   不写 Windows hosts）。
2. **证书**：为每个名称签发 TLS 证书（certbot 或云 CA），放入
   `/etc/ssl/sdkwork/api-gateway/`。
3. **nginx 站点**：启用 `docker/nginx/api.sdkwork.com.conf`
   （443 TLS + 80 重定向）：

   ```bash
   sudo mkdir -p /etc/nginx/sites-enabled/sdkwork /etc/ssl/sdkwork/api-gateway /var/www/certbot
   sudo cp docker/nginx/api.sdkwork.com.conf /etc/nginx/sites-enabled/sdkwork/
   sudo nginx -t && sudo systemctl reload nginx
   ```

4. **网关白名单/CORS**：生产 TOML 模板与 `docker/config/...toml` 已内置三个
   域名；容器部署时在 `.env` 确认（见 `docker/.env.example` 生产示例）：

   ```dotenv
   GATEWAY_ALLOWED_HOSTS=api.sdkwork.com,api.birdcoder.com,api.dtupay.com
   GATEWAY_CORS_ALLOWED_ORIGINS=https://api.sdkwork.com,https://api.birdcoder.com,https://api.dtupay.com,https://console.sdkwork.com
   ```

5. **验证**：`curl https://api.sdkwork.com/healthz` 等；网关按 Host 白名单
   校验转发头（421 拒绝未列入域名）。

## 多环境部署

同一镜像可同时跑 test 与 prod 两套独立 compose 项目，宿主端口、数据库、
Host 白名单、CORS 各自隔离（端口矩阵见 `deployments/README.md`）：

| 环境 | 项目名 | 宿主端口 → 容器 3900 | 数据库（同名 schema） | 域名 |
| --- | --- | --- | --- | --- |
| development | `gateway-dev` | 3910 | `sdkwork_ai_dev` | `api-dev.sdkwork.com` / `api-dev.birdcoder.com` / `api-dev.dtupay.com` |
| test | `gateway-test` | 3911 | `sdkwork_ai_test` | `api-test.sdkwork.com` / `api-test.birdcoder.com` / `api-test.dtupay.com` |
| staging | `gateway-staging` | 3912 | `sdkwork_ai_staging` | `api-staging.sdkwork.com` / `api-staging.birdcoder.com` / `api-staging.dtupay.com` |
| production | `gateway-prod` | 3913 | `sdkwork_ai_prod` | `api.sdkwork.com` / `api.birdcoder.com` / `api.dtupay.com` |

1. **环境文件**（宿主侧运行时配置，不入库；示例位于 `/opt/sdkwork/deploy/api-gateway/`）：
   - `test/.env`：`GATEWAY_HOST_PORT=3911`、`GATEWAY_POSTGRES_DB/USER/PASSWORD=sdkwork_ai_test`、
     `GATEWAY_POSTGRES_SCHEMA=sdkwork_ai_test`（必须与库名一致，网关池校验要求
     schema 等于 workspace database）、`GATEWAY_ALLOWED_HOSTS=api-test.*` 三域名、
     `GATEWAY_CORS_ALLOWED_ORIGINS=http://api-test.*` 三域名。
   - `prod/.env`：同上换成 3913 / `sdkwork_ai_prod` / `api.*` / `https://api.*`。
   - 四套受控模板位于 `docker/env/<environment>.env.example`。

2. **启动两套栈**（仓库根目录，`--env-file` 覆盖仓库根 `.env`）：

   ```bash
   docker compose -p gateway-test --env-file /opt/sdkwork/deploy/api-gateway/test/.env up -d
   docker compose -p gateway-prod --env-file /opt/sdkwork/deploy/api-gateway/prod/.env up -d
   ```

   `docker/postgres/init/001-create-schema.sh` 按 `POSTGRES_DB` 创建同名 schema，
   同一挂载脚本服务所有环境（首次启动迁移 46 个数据库模块，健康检查
   `start_period` 240s）。

3. **nginx 虚拟主机**（`/etc/nginx/sites-enabled/sdkwork/`，NGINX_SPEC 路径）：
   `api-test.sdkwork.com.conf` 三个 api-test.* → `127.0.0.1:3911`；
   `api-staging.sdkwork.com.conf` 三个 api-staging.* → `127.0.0.1:3912`；
   `api.sdkwork.com.conf` 三个 api.* → `127.0.0.1:3913`。

4. **域名解析**：WSL 与 Windows 的 `hosts` 均追加六域名 → `127.0.0.1`
   （WSL2 localhost 转发，Windows 浏览器可直接访问）。

5. **验证**：

   ```bash
   curl http://api-test.sdkwork.com/healthz   # -> 200（test 栈）
   curl http://api.sdkwork.com/readyz        # -> 200（prod 栈）
   curl -H "Host: unknown.example.com" http://127.0.0.1:3911/healthz  # -> 421
   ```

   环境隔离：api-test.* 打 3913、api.* 打 3911 均 421（Host 白名单按环境注入）。

## 常用运维

```bash
docker compose logs -f gateway
docker compose restart gateway
docker compose down          # 停止（数据卷保留）
docker compose down -v       # 停止并删除数据卷（慎用）
```

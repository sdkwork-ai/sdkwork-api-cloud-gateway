# Docker Standalone 快速部署（WSL Ubuntu + Windows）

本指南描述 `sdkwork-api-cloud-gateway` standalone 容器镜像的构建与 WSL 快速部署，
参考 `sdkwork-cloudrouter` 的容器组合模式。部署后通过三个测试域名访问网关：

- `http://testdockerapi.sdkwork.com`
- `http://testdockerapi.birdcoder.com`
- `http://testdockerapi.dtupay.com`

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
- 镜像清单：`dist/container-image.json`（版本、imageId、大小）

## 3. 启动容器组合

```bash
cp docker/.env.example .env   # 可选；默认值即可直接启动
docker compose up -d
docker compose ps             # 等待 gateway 健康（首次启动需迁移 46 个数据库模块）
```

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
- 首次连接全新外部库时，除 46 个 database-modules 外还需先迁移
  sdkwork-models 数据库模块（网关对非 current 的 models schema 会拒绝
  cloudrouter 模块启动）。用 cloudrouter 容器的云路由管理工具对目标库执行：

  ```bash
  # 把外部库容器/网络接入 cloudrouter 网络后，在 cloudrouter 容器内执行：
  docker exec -e SDKWORK_DATABASE_URL="postgresql://<user>:<pass>@<host>:<port>/sdkwork_ai_prod?options=-c%20search_path%3Dsdkwork_ai_prod" \
    sdkwork-cloudrouter-cloudrouter-1 \
    /opt/sdkwork/router/bin/cloudrouterctl ensure
  ```

  然后重启网关：`docker compose -f docker-compose.yml -f docker-compose.external.yml restart gateway`。
- 内置模式（默认 `docker compose up -d`）与外连模式互斥：外连时内置
  postgres/redis 不启动，网关直接连外部实例。

## 4. 配置 nginx（WSL）

```bash
sudo cp docker/nginx/testdockerapi-api-gateway.conf /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

## 5. 绑定 Windows hosts

在 Windows 的 `C:\Windows\System32\drivers\etc\hosts` 追加（WSL 中可直接编辑）：

```
127.0.0.1 testdockerapi.sdkwork.com testdockerapi.birdcoder.com testdockerapi.dtupay.com
```

## 6. 验证

```bash
curl http://testdockerapi.sdkwork.com/healthz
curl http://testdockerapi.birdcoder.com/readyz
curl http://testdockerapi.dtupay.com/healthz
```

浏览器访问 `http://testdockerapi.sdkwork.com/`（网关 API 面，如 `/app/v3/api/iam/...`）。

## 配置说明

容器配置遵循 RUNTIME_DIRECTORY_SPEC §4.5 容器范围规范：

- **配置格式**：TOML（ENVIRONMENT_SPEC §2/§9："TOML is preferred for SDKWork Rust
  services"）。配置优先级：内置默认 → TOML → 环境变量 → CLI → 密钥
  （ENVIRONMENT_SPEC §3）。
- **配置位置**：`/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml`（§4.5
  config mount）。镜像内置 production 模板为该路径的默认内容；`docker-compose.yml`
  将宿主文件 `docker/config/sdkwork-api-cloud-gateway.toml` 只读挂载覆盖之，
  修改限流/请求体/策略等环境变量未覆盖的字段无需重建镜像。
- **密钥**：`/run/secrets/sdkwork/api-gateway/<secret-name>`（§4.5 secret mount、
  §11 Secrets Standard）。本仓库不放置真实密钥；部署时把平台 secret 挂载到该目录
  并从配置或 `SDKWORK_*_FILE` 环境变量引用。禁止把密钥写进镜像层或 compose 文件。
- 部署期配置通过 `docker-compose.yml` / `.env` 注入（环境变量优先于 TOML）：
  - `SDKWORK_API_CLOUD_GATEWAY_ALLOWED_HOSTS` — 网关 Host 白名单（421 拒绝未列入域名）
  - `SDKWORK_CORS_ALLOWED_ORIGINS` — 浏览器 CORS 白名单（规范共享键）
  - `SDKWORK_DATABASE_*` — PostgreSQL 连接（compose 内置 postgres 服务）
  - `GATEWAY_HOST_PORT` — 主机映射端口（默认 3910）
- 数据卷：`postgres-data`、`gateway-data`（`/var/lib/sdkwork/api-gateway`，
  RUNTIME_DIRECTORY_SPEC §4.5 容器范围）。
- 多操作系统（Ubuntu systemd / Windows SCM / macOS launchd / 容器）的安装与配置
  规范见 [INSTALLATION_STANDARDS.md](INSTALLATION_STANDARDS.md)；Ubuntu systemd
  服务单元见 `deployments/systemd/`。

## 生产域名部署（api.sdkwork.com / api.birdcoder.com / api.dtupay.com）

同一网关容器可承载三个生产域名（nginx 按 `server_name` 分流，全部代理到
`127.0.0.1:3910`）：

1. **DNS**：三个域名解析到本机公网 IP（与测试域名不同，生产走真实 DNS，
   不写 Windows hosts）。
2. **证书**：为每个名称签发 TLS 证书（certbot 或云 CA），放入
   `/etc/ssl/sdkwork/api-gateway/`。
3. **nginx 站点**：启用 `docker/nginx/api-gateway-production.conf`
   （443 TLS + 80 重定向）：

   ```bash
   sudo mkdir -p /etc/nginx/sites-enabled/sdkwork /etc/ssl/sdkwork/api-gateway /var/www/certbot
   sudo cp docker/nginx/api-gateway-production.conf /etc/nginx/sites-enabled/sdkwork/
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

## 双环境部署（test + prod 并存）

同一镜像可同时跑 test 与 prod 两套独立 compose 项目，宿主端口、数据库、
Host 白名单、CORS 各自隔离（端口矩阵见 `deployments/README.md`）：

| 环境 | 项目名 | 宿主端口 → 容器 3900 | 数据库（同名 schema） | 域名 |
| --- | --- | --- | --- | --- |
| test | `gateway-test` | 3911 | `sdkwork_ai_test` | `testdockerapi.sdkwork.com` / `testdockerapi.birdcoder.com` / `testdockerapi.dtupay.com` |
| production | `gateway-prod` | 3912 | `sdkwork_ai_prod` | `api.sdkwork.com` / `api.birdcoder.com` / `api.dtupay.com` |

1. **环境文件**（宿主侧运行时配置，不入库；示例位于 `/opt/sdkwork/deploy/api-gateway/`）：
   - `test/.env`：`GATEWAY_HOST_PORT=3911`、`GATEWAY_POSTGRES_DB/USER/PASSWORD=sdkwork_ai_test`、
     `GATEWAY_POSTGRES_SCHEMA=sdkwork_ai_test`（必须与库名一致，网关池校验要求
     schema 等于 workspace database）、`GATEWAY_ALLOWED_HOSTS=testdockerapi.*` 三域名、
     `GATEWAY_CORS_ALLOWED_ORIGINS=http://testdockerapi.*` 三域名。
   - `prod/.env`：同上换成 3912 / `sdkwork_ai_prod` / `api.*` / `https://api.*`。

2. **启动两套栈**（仓库根目录，`--env-file` 覆盖仓库根 `.env`）：

   ```bash
   docker compose -p gateway-test --env-file /opt/sdkwork/deploy/api-gateway/test/.env up -d
   docker compose -p gateway-prod --env-file /opt/sdkwork/deploy/api-gateway/prod/.env up -d
   ```

   `docker/postgres/init/001-create-schema.sh` 按 `POSTGRES_DB` 创建同名 schema，
   同一挂载脚本服务所有环境（首次启动迁移 46 个数据库模块，健康检查
   `start_period` 240s）。

3. **nginx 虚拟主机**（`/etc/nginx/sites-enabled/sdkwork/`，NGINX_SPEC 路径）：
   `testdockerapi-api-gateway.conf` 三个 testdockerapi.* → `127.0.0.1:3911`；
   `api-gateway-http.conf` 三个 api.* → `127.0.0.1:3912`（HTTP；真实 DNS + CA
   证书就绪后改用 `docker/nginx/api-gateway-production.conf` 的 TLS 结构）。

4. **域名解析**：WSL 与 Windows 的 `hosts` 均追加六域名 → `127.0.0.1`
   （WSL2 localhost 转发，Windows 浏览器可直接访问）。

5. **验证**：

   ```bash
   curl http://testdockerapi.sdkwork.com/healthz   # -> 200（test 栈）
   curl http://api.sdkwork.com/readyz        # -> 200（prod 栈）
   curl -H "Host: unknown.example.com" http://127.0.0.1:3911/healthz  # -> 421
   ```

   环境隔离：testdockerapi.* 打 3912、api.* 打 3911 均 421（Host 白名单按环境注入）。

## 常用运维

```bash
docker compose logs -f gateway
docker compose restart gateway
docker compose down          # 停止（数据卷保留）
docker compose down -v       # 停止并删除数据卷（慎用）
```

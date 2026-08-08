# Docker Standalone 快速部署（WSL Ubuntu + Windows）

本指南描述 `sdkwork-api-cloud-gateway` standalone 容器镜像的构建与 WSL 快速部署，
参考 `sdkwork-cloudrouter` 的容器组合模式。部署后通过三个测试域名访问网关：

- `http://testapidocker.sdkwork.com`
- `http://testapidocker.birdcoder.com`
- `http://testapidocker.dtupay.com`

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

### 3.1 外连已有 PostgreSQL / Redis（不启动内置依赖）

数据库和 Redis 已存在（托管实例、既有安装）时，使用 external override 启动，
**不会创建** compose 内置的 postgres/redis 容器：

```bash
cp docker/.env.example .env
# 在 .env 配置外连连接（示例见 .env.example「External services mode」）：
#   GATEWAY_POSTGRES_HOST=db.internal.example.com   # 或 host.docker.internal
#   GATEWAY_POSTGRES_PORT=5432
#   GATEWAY_POSTGRES_DB=sdkwork_ai_prod
#   GATEWAY_POSTGRES_USER=sdkwork_ai_prod
#   GATEWAY_POSTGRES_PASSWORD=...
#   GATEWAY_POSTGRES_SSL_MODE=disable               # 需要 TLS 时改为 require
#   GATEWAY_CLOUDROUTER_REDIS_HOST=redis.internal.example.com
#   GATEWAY_CLOUDROUTER_REDIS_PORT=6379
#   GATEWAY_CLOUDROUTER_REDIS_PASSWORD=...
#   GATEWAY_CLOUDROUTER_REDIS_TLS=false
docker compose -f docker-compose.yml -f docker-compose.external.yml up -d
```

外连要求：

- 已有 PostgreSQL 中先建好规范化库与同名 schema（`docker-compose.external.yml` 的
  内置依赖已被禁用，不会自动建库）：

  ```bash
  psql -h <host> -U <admin> -c "CREATE DATABASE sdkwork_ai_prod;"
  psql -h <host> -U <admin> -d sdkwork_ai_prod -f docker/postgres/external-schema.sql
  ```

- 宿主机上运行的外部服务用 `GATEWAY_POSTGRES_HOST=host.docker.internal` /
  `GATEWAY_CLOUDROUTER_REDIS_HOST=host.docker.internal` 可达（override 已注入
  host-gateway extra host）。
- 内置模式（默认 `docker compose up -d`）与外连模式互斥：外连时内置
  postgres/redis 不启动，网关直接连外部实例。

## 4. 配置 nginx（WSL）

```bash
sudo cp docker/nginx/testapidocker-api-gateway.conf /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

## 5. 绑定 Windows hosts

在 Windows 的 `C:\Windows\System32\drivers\etc\hosts` 追加（WSL 中可直接编辑）：

```
127.0.0.1 testapidocker.sdkwork.com testapidocker.birdcoder.com testapidocker.dtupay.com
```

## 6. 验证

```bash
curl http://testapidocker.sdkwork.com/healthz
curl http://testapidocker.birdcoder.com/readyz
curl http://testapidocker.dtupay.com/healthz
```

浏览器访问 `http://testapidocker.sdkwork.com/`（网关 API 面，如 `/app/v3/api/iam/...`）。

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

## 常用运维

```bash
docker compose logs -f gateway
docker compose restart gateway
docker compose down          # 停止（数据卷保留）
docker compose down -v       # 停止并删除数据卷（慎用）
```

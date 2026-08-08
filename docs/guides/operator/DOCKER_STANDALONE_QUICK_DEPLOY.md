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

- 容器内配置：镜像内置 `config/sdkwork-api-cloud-gateway.production.toml`（生产姿态，
  允许主机名单/CORS 由环境变量覆盖，遵循 ENVIRONMENT_SPEC §11 环境变量优先）。
- 部署期配置全部通过 `docker-compose.yml` / `.env` 注入：
  - `SDKWORK_API_CLOUD_GATEWAY_ALLOWED_HOSTS` — 网关 Host 白名单（421 拒绝未列入域名）
  - `SDKWORK_CORS_ALLOWED_ORIGINS` — 浏览器 CORS 白名单（规范共享键）
  - `SDKWORK_DATABASE_*` — PostgreSQL 连接（compose 内置 postgres 服务）
  - `GATEWAY_HOST_PORT` — 主机映射端口（默认 3910）
- 数据卷：`postgres-data`、`gateway-data`（RUNTIME_DIRECTORY_SPEC §4.5 容器范围）。

## 常用运维

```bash
docker compose logs -f gateway
docker compose restart gateway
docker compose down          # 停止（数据卷保留）
docker compose down -v       # 停止并删除数据卷（慎用）
```

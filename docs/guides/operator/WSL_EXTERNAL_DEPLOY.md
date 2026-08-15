# WSL Ubuntu 外部依赖 Docker 部署（外部 PostgreSQL / Redis）

本文档描述在 WSL Ubuntu 22.04 中，用 Docker 部署 `sdkwork-api-cloud-gateway`
test 环境，但 **PostgreSQL 与 Redis 使用 Ubuntu 宿主自身安装的服务**
（外部依赖模式），不启动 compose 内置容器。部署后三个 test 域名可用：

- `http://api-test.sdkwork.com`
- `http://api-test.birdcoder.com`
- `http://api-test.dtupay.com`

## 域名约定

| 部署方式 | 域名前缀 | 说明 |
| --- | --- | --- |
| **docker 部署**（test） | `api-test.*` | 本手册场景 |
| 直跑进程（test） | `api-test.*` | WSL 直跑网关使用同一环境域名规则 |
| docker 部署（production） | `api.*` | 见 `REMOTE_DEPLOY_UBUNTU_DOCKER.md` |

## 外部依赖模式与内置模式对比

| 项 | 内置模式（默认） | 外部依赖模式（本手册） |
| --- | --- | --- |
| PostgreSQL | compose 内置 `pgvector/pgvector:pg18` 容器 | Ubuntu 宿主原生 PostgreSQL（需 pgvector 扩展） |
| Redis | compose 内置 `redis:8-alpine` 容器 | Ubuntu 宿主原生 redis-server |
| 启动命令 | `docker compose up -d` | `docker compose -f docker-compose.yml -f docker-compose.external.yml up -d` |
| 宿主端口 | 3911（test）/ 3913（production） | 相同 |

## 前置要求（WSL 宿主）

- **PostgreSQL**：`listen_addresses = '*'`（默认仅 127.0.0.1 时容器无法访问）；
  pg_hba 放行 docker 网段（172.16.0.0/12）；已安装 pgvector 扩展
  （`postgresql-18-pgvector` 等匹配大版本包，PGDG 源）；网关用户拥有
  同名 schema（`docker/postgres/external-schema.sql`）。
- **Redis**：`bind 0.0.0.0` 且 `protected-mode no`（无密码，或配置
  `GATEWAY_CLOUDROUTER_REDIS_PASSWORD`）。
- 数据库与用户：`sdkwork_ai_test`（test）/ `sdkwork_ai_prod`（production），
  各带同名 schema。

## 部署步骤

### 1. 数据库准备（每个环境一次）

```bash
# 建库（admin）
psql -h 127.0.0.1 -U postgres -c "CREATE DATABASE sdkwork_ai_test OWNER sdkwork_ai_test;"
# 同名 schema + vector 扩展 + 授权（参数化脚本，见 docker/postgres/external-schema.sql）
psql -h 127.0.0.1 -U postgres -d sdkwork_ai_test -v db=sdkwork_ai_test \
  -v app_user=sdkwork_ai_test -f docker/postgres/external-schema.sql
```

### 2. 环境文件

从受控模板生成 `/opt/sdkwork/deploy/api-gateway/external/env.test`：

```dotenv
GATEWAY_HOST_PORT=3911
GATEWAY_POSTGRES_HOST=host.docker.internal
GATEWAY_POSTGRES_PORT=5432
GATEWAY_POSTGRES_DB=sdkwork_ai_test
GATEWAY_POSTGRES_SCHEMA=sdkwork_ai_test   # 必须等于库名
GATEWAY_POSTGRES_USER=sdkwork_ai_test
GATEWAY_POSTGRES_PASSWORD=<test用户密码>
GATEWAY_POSTGRES_SSL_MODE=disable
GATEWAY_CLOUDROUTER_REDIS_HOST=host.docker.internal
GATEWAY_CLOUDROUTER_REDIS_PORT=6379
GATEWAY_CLOUDROUTER_REDIS_DATABASE=0
GATEWAY_CLOUDROUTER_REDIS_PASSWORD=
GATEWAY_ALLOWED_HOSTS=api-test.sdkwork.com,api-test.birdcoder.com,api-test.dtupay.com,localhost:3911,127.0.0.1:3911
GATEWAY_CORS_ALLOWED_ORIGINS=http://api-test.sdkwork.com,http://api-test.birdcoder.com,http://api-test.dtupay.com,http://localhost:3911,http://127.0.0.1:3911
```

### 3. 启动（外部模式）

```bash
cd /mnt/e/sdkwork-space/sdkwork-api-cloud-gateway
docker compose -p gateway-test \
  -f docker-compose.yml -f docker-compose.external.yml \
  --env-file /opt/sdkwork/deploy/api-gateway/external/env.test up -d
# 验证：应只有 gateway 容器（无内置 postgres/redis）
docker compose -p gateway-test ps
```

入口脚本会先执行显式 `--migrate-databases`，因此全新外部库无需 dump/导入
Models 表。运行时 `SDKWORK_DATABASE_AUTO_MIGRATE=false`，迁移仅由启动前的
单进程生命周期步骤执行。

### 5. nginx 站点 + hosts

```bash
sudo cp docker/nginx/api-test.sdkwork.com.conf /etc/nginx/sites-enabled/sdkwork/
sudo nginx -t && sudo systemctl reload nginx
# hosts（WSL 与 Windows 均追加）
127.0.0.1 api-test.sdkwork.com api-test.birdcoder.com api-test.dtupay.com
```

## 验证

```bash
curl http://api-test.sdkwork.com/healthz     # 200
curl http://api-test.birdcoder.com/readyz    # 200
curl http://api-test.dtupay.com/healthz      # 200
# Host 白名单（421）
curl -s -o /dev/null -w "%{http_code}\n" -H "Host: evil.example.com" http://127.0.0.1:3911/healthz
# 直连宿主库确认归属
psql -h 127.0.0.1 -U sdkwork_ai_test -d sdkwork_ai_test -c "\dn"
```

## 运维

```bash
# 停止（保留宿主库数据）
docker compose -p gateway-test -f docker-compose.yml -f docker-compose.external.yml down
# 升级：重建镜像后
docker compose -p gateway-test -f docker-compose.yml -f docker-compose.external.yml \
  --env-file /opt/sdkwork/deploy/api-gateway/external/env.test up -d
# 回退内置模式：停外部栈后按 DOCKER_STANDALONE_QUICK_DEPLOY.md 默认命令启动
docker compose -p gateway-test up -d
```

升级或首次部署前可先运行
`pnpm check:container-deployment -- --env-file <env-file> --mode external --compose`
验证外部依赖、环境身份、域名和 Compose 合并结果。

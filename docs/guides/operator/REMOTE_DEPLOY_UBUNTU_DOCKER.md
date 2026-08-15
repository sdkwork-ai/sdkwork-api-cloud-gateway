# 远程 Ubuntu 服务器部署（Docker 方式，CI 预留）

本文档描述将 `sdkwork-api-cloud-gateway` 双环境（test / prod）部署到远程
Ubuntu 服务器的方法：**本机构建容器镜像 → 传输 → 服务器 docker compose 编排**，
并预留 GitHub Actions（`sdkwork.workflow.json`）发布路径作为正式迭代方式。

## 部署总览

| 环境 | Compose 项目 | 宿主端口 → 容器 3900 | 数据库（同名 schema） | 域名 |
| --- | --- | --- | --- | --- |
| test | `gateway-test` | 3911 | `sdkwork_ai_test` | `api-test.sdkwork.com` / `api-test.birdcoder.com` / `api-test.dtupay.com` |
| production | `gateway-prod` | 3913 | `sdkwork_ai_prod` | `api.sdkwork.com` / `api.birdcoder.com` / `api.dtupay.com` |

服务器仅需 **Docker Engine + nginx**；PostgreSQL（pgvector）与 Redis 由
compose 内置，无 Rust/Node 依赖。

## 依赖部署三模式（内置 / 外连远端 / Ubuntu 宿主原生）

PostgreSQL 与 Redis 的依赖来源三选一，**同一套 compose 同时支持**，切换只改
启动命令与 `.env` 连接值：

| 模式 | 依赖来源 | 启动命令 | .env 关键值 |
| --- | --- | --- | --- |
| ① 内置（默认） | compose 内置 pgvector + Redis 容器 | `docker compose up -d` | 无需设置（默认 `postgres`/`redis` 主机名） |
| ② 外连远端 | 已有远程/托管 PostgreSQL、Redis | `docker compose -f docker-compose.yml -f docker-compose.external.yml up -d` | `GATEWAY_POSTGRES_HOST=<远端IP>`、`GATEWAY_CLOUDROUTER_REDIS_HOST=<远端IP>` |
| ③ Ubuntu 宿主原生 | 服务器 apt 已装 `postgresql`/`redis-server`（无容器） | 同上 external override | `GATEWAY_POSTGRES_HOST=host.docker.internal`、`GATEWAY_CLOUDROUTER_REDIS_HOST=host.docker.internal` |

`docker-compose.external.yml` 将内置 postgres/redis 移入永不启用的
`external-disabled` profile，并注入 `host-gateway` extra host，因此 ②③
共用同一 override；两种姿态只差 `.env` 里的 host 值与宿主侧服务准备。

> WSL 本机的外部依赖模式完整演练与验证清单见
> `WSL_EXTERNAL_DEPLOY.md`。test 环境统一使用 `api-test.*`。

### 模式③ Ubuntu 宿主原生服务准备（一次性）

```bash
# 1) pgvector 扩展（knowledgebase 模块强依赖；与内置镜像同为主流版本 PG18，
#    版本按服务器 PostgreSQL 大版本选包，官方源缺失时用 PGDG 源）
sudo apt-get install -y postgresql-18-pgvector   # 或 postgresql-14/15/16/17-pgvector

# 2) PostgreSQL 允许 docker 网段访问（host-gateway 经宿主 eth0 到达，
#    默认只监听 127.0.0.1 无法被容器访问）
sudo sed -i "s/^#\?listen_addresses.*/listen_addresses = '*'/" \
  /etc/postgresql/*/main/postgresql.conf
# 追加 pg_hba.conf（Docker 默认网段 172.16.0.0/12）：
echo "host all all 172.16.0.0/12 scram-sha-256" | sudo tee -a \
  /etc/postgresql/*/main/pg_hba.conf
sudo systemctl restart postgresql

# 3) Redis 允许容器访问（默认仅 bind 127.0.0.1）
sudo sed -i "s/^bind 127.0.0.1 -::1$/bind 0.0.0.0 -::1/" /etc/redis/redis.conf
sudo sed -i "s/^protected-mode yes$/protected-mode no/" /etc/redis/redis.conf
sudo systemctl restart redis-server

# 4) 建库 + 同名 schema + 授权（每个环境执行一次，脚本参数化见
#    docker/postgres/external-schema.sql）
sudo -u postgres psql -c "CREATE ROLE sdkwork_ai_test LOGIN PASSWORD 'change-me';"
sudo -u postgres psql -c "CREATE DATABASE sdkwork_ai_test OWNER sdkwork_ai_test;"
sudo -u postgres psql -d sdkwork_ai_test -v db=sdkwork_ai_test \
  -v app_user=sdkwork_ai_test -f docker/postgres/external-schema.sql
# prod 库同理：sdkwork_ai_prod
```

### 模式② 外连远端库准备（一次性）

```bash
psql -h <远端IP> -U <admin> -c "CREATE DATABASE sdkwork_ai_test;"
psql -h <远端IP> -U <admin> -d sdkwork_ai_test -v db=sdkwork_ai_test \
  -v app_user=sdkwork_ai_test -f docker/postgres/external-schema.sql
# prod 库同理；远端 Redis 需可被 docker 宿主机访问
```

### 模式③ .env 连接值

```dotenv
# 每个环境的环境文件里追加/覆盖：
GATEWAY_POSTGRES_HOST=host.docker.internal
GATEWAY_POSTGRES_PORT=5432
GATEWAY_POSTGRES_DB=sdkwork_ai_test          # prod: sdkwork_ai_prod
GATEWAY_POSTGRES_SCHEMA=sdkwork_ai_test      # 必须等于库名
GATEWAY_POSTGRES_USER=sdkwork_ai_test
GATEWAY_POSTGRES_PASSWORD=<宿主角色的密码>
GATEWAY_POSTGRES_SSL_MODE=disable

GATEWAY_CLOUDROUTER_REDIS_HOST=host.docker.internal
GATEWAY_CLOUDROUTER_REDIS_PORT=6379
GATEWAY_CLOUDROUTER_REDIS_DATABASE=0
GATEWAY_CLOUDROUTER_REDIS_PASSWORD=
```

> 宿主原生模式下 `docker compose ... up -d` 不会创建 postgres/redis 容器，
> 健康检查由 external override 解除依赖；入口脚本会在服务启动前显式迁移
> 全部数据库模块，包括 Models 与 CloudRouter。

## 一、服务器准备

```bash
# Docker Engine（Ubuntu 22.04）
sudo apt-get update && sudo apt-get install -y docker.io docker-compose-plugin
sudo systemctl enable --now docker
sudo usermod -aG docker "$USER"     # 重新登录生效

# nginx（域名分流）
sudo apt-get install -y nginx

# 目录布局
sudo mkdir -p /opt/sdkwork/deploy/api-gateway/{test,prod}
sudo chown -R "$USER":"$USER" /opt/sdkwork/deploy/api-gateway
```

## 二、本机构建与传输（WSL/构建机）

```bash
# 1) 构建镜像（复用已有 release 二进制；镜像含 51 个数据库模块）
cd /mnt/e/sdkwork-space/sdkwork-api-cloud-gateway
pnpm build:container --skip-build

# 2) 导出并压缩（镜像约 1.5GB，gzip 后约 600-800MB）
docker save sdkwork-api-cloud-gateway:local | gzip > gateway-image.tar.gz

# 3) 传输清单
#    - gateway-image.tar.gz           容器镜像
#    - docker-compose.yml             双环境共用编排（容器内 3900，宿主 3911/3913）
#    - docker/                        配置挂载（config/、postgres/init/）
#    - 双环境 .env（见第四节模板，密码在服务器本地生成）
scp gateway-image.tar.gz docker-compose.yml user@server:/opt/sdkwork/gateway/
scp -r docker user@server:/opt/sdkwork/gateway/
```

> 镜像升级时重复 1-3 步，服务器端 `docker load` 后
> `docker compose ... up -d --force-recreate`（数据卷保留，迁移自动执行）。

## 三、服务器加载镜像

```bash
cd /opt/sdkwork/gateway
gzip -dc gateway-image.tar.gz | docker load
docker images sdkwork-api-cloud-gateway:local
```

## 四、双环境 .env（服务器本地生成，密钥不入库）

```bash
DB_PASS_TEST=$(openssl rand -hex 16)
DB_PASS_PROD=$(openssl rand -hex 16)
```

`/opt/sdkwork/deploy/api-gateway/test/.env`：

```dotenv
# sdkwork-api-cloud-gateway docker compose environment — test (api-test.*).
GATEWAY_HOST_PORT=3911
GATEWAY_POSTGRES_DB=sdkwork_ai_test
GATEWAY_POSTGRES_SCHEMA=sdkwork_ai_test   # 必须等于库名（进程池校验）
GATEWAY_POSTGRES_USER=sdkwork_ai_test
GATEWAY_POSTGRES_PASSWORD=<DB_PASS_TEST>
GATEWAY_ALLOWED_HOSTS=api-test.sdkwork.com,api-test.birdcoder.com,api-test.dtupay.com,localhost:3911,127.0.0.1:3911
GATEWAY_CORS_ALLOWED_ORIGINS=http://api-test.sdkwork.com,http://api-test.birdcoder.com,http://api-test.dtupay.com,http://localhost:3911,http://127.0.0.1:3911
```

`/opt/sdkwork/deploy/api-gateway/prod/.env`：

```dotenv
# sdkwork-api-cloud-gateway docker compose environment — production (api.*).
GATEWAY_HOST_PORT=3913
GATEWAY_POSTGRES_DB=sdkwork_ai_prod
GATEWAY_POSTGRES_SCHEMA=sdkwork_ai_prod
GATEWAY_POSTGRES_USER=sdkwork_ai_prod
GATEWAY_POSTGRES_PASSWORD=<DB_PASS_PROD>
GATEWAY_ALLOWED_HOSTS=api.sdkwork.com,api.birdcoder.com,api.dtupay.com,localhost:3913,127.0.0.1:3913
GATEWAY_CORS_ALLOWED_ORIGINS=https://api.sdkwork.com,https://api.birdcoder.com,https://api.dtupay.com,https://console.sdkwork.com,http://localhost:3913,http://127.0.0.1:3913
```

> 必改密钥（非测试环境）：`GATEWAY_CLOUDROUTER_API_KEY_PEPPER`、
> `GATEWAY_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING`、
> `GATEWAY_CLOUDROUTER_*_SECRET`（在 .env 追加覆盖 compose 默认值，
> 用 `openssl rand -base64 32` 生成；key ring 为 camelCase JSON，键长 ≥32 字节）。

## 五、启动双环境

```bash
cd /opt/sdkwork/gateway
docker compose -p gateway-test --env-file /opt/sdkwork/deploy/api-gateway/test/.env up -d
docker compose -p gateway-prod --env-file /opt/sdkwork/deploy/api-gateway/prod/.env up -d
# 首次启动迁移 46+ 个数据库模块，等待 healthy（start_period 240s）
docker compose -p gateway-test ps
docker compose -p gateway-prod ps
```

## 六、数据库生命周期

容器入口先执行 `sdkwork-api-cloud-gateway --migrate-databases`，再启动服务。
该显式步骤迁移完整模块清单，包括 Models 与 CloudRouter；服务运行阶段保持
`SDKWORK_DATABASE_AUTO_MIGRATE=false`。多副本或外部发布系统应选举一个迁移任务，
并对普通副本设置 `GATEWAY_MIGRATE_ON_START=false`。

## 七、nginx 域名分流（服务器）

启用 `docker/nginx/api-test.sdkwork.com.conf`（三个 api-test.* →
`127.0.0.1:3911`）与 `docker/nginx/api.sdkwork.com.conf`（三个 api.* →
`127.0.0.1:3913`）。生产证书放在模板声明的受保护路径。

```bash
sudo nginx -t && sudo systemctl reload nginx
```

## 八、DNS 与自测

- `api.*` 三域名 A 记录指向服务器公网 IP（真实上线必需）；`api-test.*` 可先
  只做服务器本机 `/etc/hosts` 自测
- 服务器 `/etc/hosts` 追加六域名 → `127.0.0.1`（对外发布后移除 test 条目）
- 服务器防火墙放行 80/443（及必要的 3911/3913 管理端口）

## 九、验证清单

```bash
# 容器健康
docker compose -p gateway-test ps          # 全 healthy
# 六域名（服务器本机）
for d in api-test.sdkwork.com api-test.birdcoder.com api-test.dtupay.com \
         api.sdkwork.com api.birdcoder.com api.dtupay.com; do
  curl -s -o /dev/null -w "%{http_code} $d\n" http://$d/healthz
done
# Host 白名单（421）
curl -s -o /dev/null -w "%{http_code}\n" -H "Host: evil.example.com" http://127.0.0.1:3911/healthz
curl -s -o /dev/null -w "%{http_code}\n" -H "Host: api-test.sdkwork.com" http://127.0.0.1:3913/healthz
# CORS 预检（允许 Origin 回显 allow-origin；禁止的不回显）
curl -s -D - -o /dev/null -X OPTIONS -H "Origin: http://api-test.sdkwork.com" \
  -H "Access-Control-Request-Method: GET" http://api-test.sdkwork.com/healthz | grep -i allow-origin
# API 面（规范化 401 结构，非 5xx）
curl -s http://api.sdkwork.com/backend/v3/api/iam/users | head -c 200
# 库隔离
docker exec gateway-test-postgres-1 psql -U sdkwork_ai_test -d sdkwork_ai_test -c "\dn"
```

## 十、CI 预留（方案 C 切换点）

`pnpm gateway:package:standalone` / `sdkwork.workflow.json` 发布后：

1. CI 产出 `sdkwork-api-cloud-gateway-<v>-linux-x64-standalone-server.tar.gz`
   与 OCI 镜像（`registry.sdkwork.com/sdkwork-api-cloud-gateway`）
2. 服务器改为拉取发布产物：
   ```bash
   docker pull registry.sdkwork.com/sdkwork-api-cloud-gateway:<tag>
   docker tag registry.sdkwork.com/sdkwork-api-cloud-gateway:<tag> sdkwork-api-cloud-gateway:local
   docker compose -p gateway-test --env-file ... up -d --force-recreate
   ```
3. 安装包模式（可选）按 `deployments/systemd/README.md` 走 systemd 部署
4. 发布前用 `dist/` 内 `checksums.sha256` 校验产物完整性

## 常用运维

```bash
docker compose -p gateway-test logs -f gateway
docker compose -p gateway-prod restart gateway
docker compose -p gateway-test down          # 停止（数据卷保留）
docker compose -p gateway-prod down -v       # 停止并删数据卷（慎用）
```

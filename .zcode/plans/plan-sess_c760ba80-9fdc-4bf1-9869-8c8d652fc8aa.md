## 目标：WSL 中 test 环境用 Docker（外部 PostgreSQL/Redis 模式）部署，域名统一为 testdockerapi.* 前缀，打磨完整可复用的部署能力

### 域名约定（修正）
- **docker 部署 test 环境**：`testdockerapi.sdkwork.com` / `testdockerapi.birdcoder.com` / `testdockerapi.dtupay.com`（testdocker 开头，取代旧 testapi.*/testapidocker.* 的 docker 约定）
- prod（api.*）与 3910 dev 栈不受影响

### 现状（已调研确认）
- 宿主原生 PostgreSQL 18.4（listen '*'、pgvector 0.8.3 已装、`sdkwork_ai_prod` 就绪；`sdkwork_ai_test` 库存在但 test 用户密码未知——先用直跑脚本遗留密码 `sdkworkdev123` 验证，无效则用 `.env.postgres` admin 凭据重置）
- 宿主原生 Redis 8.6.1（无密码、全接口，host-gateway 直接可用）
- 现有内置模式 gateway-test 栈（3911，testapi.*）→ 停止（保留卷可回退），由外部模式 testdockerapi.* 栈接管 3911

### 执行步骤

1. **仓库命名统一（打磨部署能力）**：
   - `docker-compose.yml` 默认 `GATEWAY_ALLOWED_HOSTS` / `GATEWAY_CORS_ALLOWED_ORIGINS`：`testapidocker.*`/`testapi.*` → `testdockerapi.*`（3910 dev 栈用仓库根 .env，不受默认值影响）
   - `docker/.env.example`：同步 testdockerapi.*
   - `docker/nginx/testapidocker-api-gateway.conf` → 新增 `testdockerapi-api-gateway.conf`（testdockerapi.* 三域名 → 3911），删除旧命名文件

2. **宿主 test 库准备**：验证/重置 `sdkwork_ai_test` 凭据；`external-schema.sql` 参数化执行（`-v db=sdkwork_ai_test`）建同名 schema + vector 扩展 + 授权

3. **外部模式部署文件**（宿主侧 `/opt/sdkwork/deploy/api-gateway/external/`，不入库）：
   - `env.test`：`GATEWAY_HOST_PORT=3911`、`GATEWAY_POSTGRES_HOST=host.docker.internal:5432`、DB/SCHEMA/USER=sdkwork_ai_test、`GATEWAY_CLOUDROUTER_REDIS_HOST=host.docker.internal:6379`、白名单/CORS=testdockerapi.* 三域名
   - `prepare-models.sh`：从内置 gateway-test 栈库 pg_dump 16 张 ai_model 表 → 导入宿主 test 库

4. **切换部署**：
   - 停内置 gateway-test 栈（`down` 保留卷）
   - `docker compose -p gateway-test -f docker-compose.yml -f docker-compose.external.yml --env-file .../env.test up -d`（无内置 postgres/redis 容器，网关连宿主 5432/6379）

5. **nginx + hosts**：
   - 新站点 `sdkwork/testdockerapi-api-gateway.conf`（testdockerapi.* → 3911），旧 testapi 站点停用（.orig）
   - WSL + Windows hosts 追加 testdockerapi.* 三域名 → 127.0.0.1

6. **runbook**：新增 `docs/guides/operator/WSL_EXTERNAL_DEPLOY.md`（外部模式完整步骤：凭据、库准备、models 导入、启动/停止/升级、回退内置模式），`REMOTE_DEPLOY_UBUNTU_DOCKER.md` 补充引用

7. **全量验证**：
   - gateway-test 栈无内置 postgres/redis 容器；网关连宿主 5432/6379
   - testdockerapi.* 三域名 × healthz/readyz = 200；421 白名单（未知/跨环境）；CORS 抽测
   - 宿主 test 库 models 数据（187/25）+ 迁移记录；3910/prod 栈不受影响
   - `check-source-config-standard` / compose config 校验

### 不做
- prod（api.*）保持内置模式；git 不提交（改动留工作区）
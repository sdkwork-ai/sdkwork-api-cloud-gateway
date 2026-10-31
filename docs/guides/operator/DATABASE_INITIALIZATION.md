# 数据库初始化（安装后快速落地）

sdkwork-api-cloud-gateway 会装配 component contract 声明的 foundation 数据库模块，
并额外迁移 CloudRouter 依赖的 Models 模块。安装/启动流程现在对所有生命周期环境
（development / test / staging / demo / production）执行完整的三阶段初始化：

```text
--provision-databases  →  --bootstrap-databases  →  --migrate-databases  →  启动网关
  角色/数据库/schema/      每个模块的 ops 历史表、      owner assembly 装配 +
  扩展/授权（管理员级）    baseline、增量迁移          drift/完整性校验
```

- 容器入口（`bin/container/entrypoint`）与 systemd
  `ExecStartPre` 都会按 provision → bootstrap → migrate 顺序执行；三个阶段都幂等。
  bootstrap 只建模块 schema，不组装 router，因此可以在 Knowledgebase RPC
  sidecar 启动前完成；sidecar 校验 drive/knowledgebase 表后，migrate 才能完成
  IM/CloudRouter 等 owner assembly。`test/staging/demo/production` 首次安装必须
  先执行 bootstrap，再启动 sidecar。
- `pnpm dev` 仍由 Node 开发编排器调用同一套工作区初始化逻辑
  （`sdkwork-specs/tools/postgres/postgres-init-node.mjs`）后再启动网关。
- 生产/类生产环境保持 `[database] autoMigrate = false`：运行时不隐式改 schema，
  显式命令才是授权。

## 快速开始（3 步）

```bash
# 1. 配置数据库连接（环境变量或 .env）
export SDKWORK_DATABASE_HOST=127.0.0.1
export SDKWORK_DATABASE_PORT=5432
export SDKWORK_DATABASE_NAME=sdkwork_ai_prod        # 规范库名：dev/test/staging/prod
export SDKWORK_DATABASE_SCHEMA=sdkwork_ai_prod
export SDKWORK_DATABASE_USERNAME=sdkwork_ai_prod
export SDKWORK_DATABASE_PASSWORD=...

# 管理员连接（预建库/schema/扩展用；fresh 安装或需要收敛角色密码时必需）
export SDKWORK_DATABASE_ADMIN_USERNAME=postgres
export SDKWORK_DATABASE_ADMIN_PASSWORD=...
# 或用文件形式：export SDKWORK_DATABASE_ADMIN_PASSWORD_FILE=/etc/sdkwork/database/admin.secret

# 2. 启动安装后的网关（容器/systemd 自动完成 provision + bootstrap + migrate）
./bin/sdkwork-api-cloud-gateway

# 3. 验证
curl http://127.0.0.1:3900/readyz
```

手工分步执行（排障或独立发布作业）时：

```bash
./bin/sdkwork-api-cloud-gateway --provision-databases
./bin/sdkwork-api-cloud-gateway --bootstrap-databases   # 先建所有模块表
# 启动 Knowledgebase RPC sidecar（test/staging/demo/production 必需）
./bin/sdkwork-api-cloud-gateway --migrate-databases     # 再装配 owner router
pnpm db:init          # 兼容的 Node 初始化器（psql；建库/schema/扩展/授权，不创建角色）
pnpm db:postgres:provision   # 宿主 PostgreSQL 角色/库/schema 供给（WSL 外部依赖模式）
```

## 初始化机制

| 阶段 | 执行者 | 内容 | 幂等 |
| --- | --- | --- | --- |
| 管理员准备 | `--provision-databases` | 角色、数据库、schema、扩展（vector/pg_trgm，可选 zhparser/pg_bigm）、schema/默认权限授权、角色密码与 search_path 收敛 | 是（存在性检查 + IF NOT EXISTS） |
| 模块 schema | `--bootstrap-databases` | 每个模块：ops 历史表、baseline、增量迁移与 drift 检查（含 Models/CloudRouter 依赖表），不组装 router/RPC | 是（ops_* 状态记录） |
| Owner 装配 | `--migrate-databases` | 再次幂等 discovery，然后装配 IM/CloudRouter 等 owner router 并校验完整性 | 是（零新增迁移） |
| 服务启动 | 网关服务进程 | 连接并校验已迁移 schema，不在生产运行时改变 schema | 是 |

- **安装器编排**：容器 entrypoint 与 systemd `ExecStartPre` 自动执行
  provision → bootstrap → migrate；服务运行时使用
  `SDKWORK_DATABASE_AUTO_MIGRATE=false`。Knowledgebase RPC sidecar 在 bootstrap
  之后启动；容器重启策略会吸收首次启动时的短暂等待。
- **多实例**：实例 1 拥有 provision + migrate；实例 2..N 以
  `SDKWORK_API_CLOUD_GATEWAY_MIGRATE_ON_START=false` 启动，避免并发 DDL。
- **开发直跑**：development/test TOML 可保留 `autoMigrate=true` 以便本地单进程开发；
  staging/production 必须走显式命令。
- **连接预算**：foundation suite 首次装配会同时创建 55+ 模块及临时驱动池。
  容器/进程安装默认写入 `SDKWORK_DATABASE_MAX_CONNECTIONS=150`（回归验证的最低值）；
  PostgreSQL `max_connections` 必须按
  `sum(process pool maxima) + migration/admin reserve + operator reserve`
  预留，embedded 默认 400，外部库也应按此扩容。
- **角色/库/schema**：应用凭据在角色已拥有数据库时即可完成 schema/授权/扩展收敛；
  fresh 主机需要 `SDKWORK_DATABASE_ADMIN_*`（或 `SDKWORK_DATABASE_ADMIN_URL`）
  创建角色与数据库并收敛密码。未配置管理员时必须由运维预建，否则启动 fail-closed
  并给出明确修复命令。
- **扩展**：`vector`（knowledgebase）与 `pg_trgm`（search）在装配了对应
  surface 时是必需项；`zhparser`/`pg_bigm` 为可选项。需要超级用户安装的扩展由
  provision 阶段用管理员连接预建到业务 schema；必需扩展缺失时会阻止启动并提示
  安装服务器扩展包。
- **sdkwork-models**：Models host 的 canonical baseline 与 migrations 由
  `--bootstrap-databases` 提前执行，`--migrate-databases` 的 CloudRouter owner
  assembly 只做完整性校验；不再依赖 dump、手工建表或从其他环境导入。
- **显式关闭**：运维自管 schema 生命周期时可在部署 env 中设置
  `SDKWORK_DATABASE_PROVISION=false`（容器内）或
  `GATEWAY_DATABASE_PROVISION=false`（bundle env），跳过 provision 阶段。

## 管理员连接环境变量

标准工作区变量（`ENVIRONMENT_SPEC.md` §7.1）：

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `SDKWORK_DATABASE_ADMIN_URL` | 空 | 完整管理员 URL，优先于拆分字段 |
| `SDKWORK_DATABASE_ADMIN_HOST` | 应用 host | 管理员连接主机 |
| `SDKWORK_DATABASE_ADMIN_PORT` | 应用 port | 管理员连接端口 |
| `SDKWORK_DATABASE_ADMIN_DATABASE` | `postgres` | 管理员连接维护库 |
| `SDKWORK_DATABASE_ADMIN_USERNAME` | 应用用户 | 需 CREATEROLE/CREATEDB（扩展另需超级用户） |
| `SDKWORK_DATABASE_ADMIN_PASSWORD` / `..._PASSWORD_FILE` | 应用密码 | 二选一；文件形式优先 |
| `SDKWORK_DATABASE_ADMIN_SSL_MODE` | 应用 SSL mode | disable/allow/prefer/require/verify-ca/verify-full |

容器部署通过宿主侧 `GATEWAY_POSTGRES_ADMIN_*` 键映射（见
`docker/env/<environment>.env.example`）；systemd 部署可直接把
`SDKWORK_DATABASE_ADMIN_*` 写入 `/etc/sdkwork/api-gateway/api-gateway.env`
（安装器也会从安装进程环境同步这些键）。

## 嵌入装配依赖环境变量

部分内嵌装配在启动时直接读取环境变量（不经网关 TOML）。Docker Compose 与部署脚本
已内置下列默认值；**裸二进制部署需按表设置**，否则启动会在对应模块失败：

| 环境变量 | 示例值 | 缺失后果 |
| --- | --- | --- |
| `SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION` | `true` | voice 等模块请求 AnyPool 时与进程共享池驱动不匹配，启动失败 |
| `SDKWORK_RTC_HYDRATE_TENANT_ID` | `100001` | RTC 持久化 hydrate 失败（表列为 BIGINT，字符串 `default` 无法解析） |
| `SDKWORK_RTC_HYDRATE_ORGANIZATION_ID` | `0` | 同上 |
| `SDKWORK_KNOWLEDGEBASE_TENANT_ID` | `100001` | knowledgebase hydrate 失败 |
| `SDKWORK_KNOWLEDGEBASE_ORGANIZATION_ID` | `0` | 同上 |
| `SDKWORK_KNOWLEDGEBASE_ENVIRONMENT` | `development` | 非 development 时必须显式设置 tenant/org |
| `SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID` | `1` | webserver 主键生成未配置（多实例需唯一） |

云网关实时平面（ADR-20260809-platform-gateway-realtime-hosting）额外使用
`SDKWORK_IM_REALTIME_*` 环境族（BIND/URL/TLS），详见
`APP_RUNTIME_TOPOLOGY_NAMING.md`。

## 跨操作系统与打包方式

| 部署方式 | provision 前置 | 启动初始化 |
| --- | --- | --- |
| WSL/Ubuntu 直部署 | 应用角色已存在或配置管理员 | `--provision-databases` → `--bootstrap-databases` → 启动 RPC sidecar → `--migrate-databases` 后启动 |
| Linux/macOS 直部署 | 同上 | 同上 |
| Windows 直部署 | 同上（psql 仅在 `pnpm db:init` 时需要） | 同上 |
| Docker Compose / bundle | 外部模式：配置管理员或预建库；embedded 模式自动 | entrypoint 显式 provision + bootstrap + migrate；sidecar 重启策略吸收首次等待 |
| systemd | 应用角色已存在或配置管理员 | 三个 `ExecStartPre` 后启动 |

打包产物（standalone tar.gz / 容器镜像）内嵌各模块的 database 契约与迁移文件；
`--provision-databases` 只处理工作区级对象（角色/库/schema/扩展）；模块表生命周期
由 `--bootstrap-databases` 的 discovery 负责，`--migrate-databases` 完成 owner
assembly 装配与校验，符合 `DATABASE_FRAMEWORK_SPEC.md` §4.1/§4.4 的分层。

## 验证命令

```bash
curl http://127.0.0.1:3900/readyz     # 200 即数据层就绪
# 检查模块表（示例）
psql -d sdkwork_ai_prod -c "SELECT count(*) FROM information_schema.tables
  WHERE table_schema = 'sdkwork_ai_prod'"
# 迁移历史
psql -d sdkwork_ai_prod -c "SELECT module_id, count(*) FROM
  sdkwork_ai_prod.ops_schema_migration_history GROUP BY 1 ORDER BY 1"
```

## 故障排查

- **启动报数据库不存在/角色不存在**：配置 `SDKWORK_DATABASE_ADMIN_*`
  后重启；provision 阶段会创建并授权。也可预先运行
  `--provision-databases` 或 `pnpm db:postgres:provision`。
- **启动报扩展缺失**：用超级用户执行 `CREATE EXTENSION IF NOT EXISTS <name>
  WITH SCHEMA <schema>`，安装服务器扩展包（如 `postgresql-XX-pgvector`），
  或重跑 `--provision-databases`。
- **角色密码不匹配**：配置管理员后重启，provision 会用部署 env 中的密码收敛角色；
  或手工 `ALTER ROLE <user> PASSWORD '<env 中的密码>'`。
- **迁移 checksum 不匹配**：迁移文件在构建/安装后被修改，或部署了不一致的
  artifact。停止发布并核对包内 `checksums.sha256` 与版本；迁移历史修复需要人工评审。
- **连接数不足**：`remaining connection slots` 表示 PostgreSQL
  `max_connections` 上限；调大服务端配置，并保持每实例连接池
  （`SDKWORK_DATABASE_MAX_CONNECTIONS`）与总预算匹配。

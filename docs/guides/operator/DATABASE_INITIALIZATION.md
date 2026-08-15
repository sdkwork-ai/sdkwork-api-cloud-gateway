# 数据库初始化（安装后快速落地）

sdkwork-api-cloud-gateway 会装配 component contract 声明的 foundation 数据库模块，
并额外迁移 CloudRouter 依赖的 Models 模块。安装流程为：`db:init`（管理员级准备）
→ 显式 `--migrate-databases` → 启动网关。迁移步骤幂等，升级时应用增量迁移。

## 快速开始（3 步）

```bash
# 1. 配置数据库连接（环境变量或 .env）
export SDKWORK_DATABASE_HOST=127.0.0.1
export SDKWORK_DATABASE_PORT=5432
export SDKWORK_DATABASE_NAME=sdkwork_ai_prod        # 规范库名：dev/test/staging/prod
export SDKWORK_DATABASE_SCHEMA=sdkwork_ai_prod
export SDKWORK_DATABASE_USERNAME=sdkwork_ai_prod
export SDKWORK_DATABASE_PASSWORD=...
# 管理员连接（预建库/schema/扩展用；仅 db:init 需要）
export SDKWORK_DATABASE_ADMIN_USERNAME=postgres
export SDKWORK_DATABASE_ADMIN_PASSWORD=...

# 2. 管理员级初始化（建库、建 schema、建扩展 vector/pg_trgm 等、授权业务用户）
pnpm db:init          # = node scripts/init-database.mjs

# 3. 显式迁移全部模块，再启动网关
./bin/sdkwork-api-cloud-gateway --migrate-databases
./bin/sdkwork-api-cloud-gateway
```

启动完成后 `/readyz` 返回 200，即数据层商用就绪。

## 初始化机制

| 阶段 | 执行者 | 内容 | 幂等 |
| --- | --- | --- | --- |
| 管理员准备 | `pnpm db:init` | 建库/建 schema、扩展（vector/pg_trgm/zhparser/pg_bigm）、schema 授权 | 是（IF NOT EXISTS / 状态检查） |
| 模块迁移 | `--migrate-databases` | 每个模块：ops 历史表、baseline、增量迁移与 drift 检查，包括 Models/CloudRouter | 是（ops_* 状态记录） |
| 服务启动 | 网关服务进程 | 连接并校验已迁移 schema，不在生产运行时改变 schema | 是 |

- **安装器编排**：容器 entrypoint 与 systemd `ExecStartPre` 自动调用显式迁移命令；
  服务运行时使用 `SDKWORK_DATABASE_AUTO_MIGRATE=false`。
- **开发直跑**：development/test TOML 可保留 `autoMigrate=true` 以便本地单进程开发；
  staging/production 必须走显式迁移。
- **扩展**：`vector`（knowledgebase）与 `pg_trgm` 等需要超级用户安装；`db:init`
  用管理员连接预建到业务 schema（模块查询的 search_path 可直接解析类型）。
  系统未安装的扩展（如 zhparser/pg_bigm）跳过并提示，不影响其它模块。
- **sdkwork-models**：Models host 的 canonical baseline 与 migrations 由同一个显式
  生命周期命令执行；不再依赖 dump、手工建表或从其他环境导入。

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

| 部署方式 | db:init 前置 | 启动初始化 |
| --- | --- | --- |
| WSL/Ubuntu 直部署 | 创建数据库、同名 schema 与扩展 | `--migrate-databases` 后启动 |
| Linux/macOS 直部署 | 同上 | 同上 |
| Windows 直部署 | 同上（psql 需在 PATH） | `--migrate-databases` 后启动 |
| Docker Compose | 内置模式自动准备；外部模式先建库/schema/扩展 | entrypoint 显式迁移后启动 |
| systemd | 先配置连接与管理员准备 | `ExecStartPre` 显式迁移后启动 |

打包产物（standalone tar.gz / 容器镜像）内嵌各模块的 database 契约与迁移文件；
`db:init` 从安装包 `database-modules/` 解析 baseline（源码检出时回退到兄弟仓库）。

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

- **启动报扩展缺失**：用超级用户执行 `CREATE EXTENSION IF NOT EXISTS <name> WITH SCHEMA <schema>`，
  或重跑 `pnpm db:init`。
- **迁移 checksum 不匹配**：迁移文件在构建/安装后被修改，或部署了不一致的
  artifact。停止发布并核对包内 `checksums.sha256` 与版本；迁移历史修复需要人工评审。
- **连接数不足**：`remaining connection slots` 表示 PostgreSQL
  `max_connections` 上限；调大服务端配置，并保持每实例连接池
  （`SDKWORK_DATABASE_MAX_CONNECTIONS`）与总预算匹配。

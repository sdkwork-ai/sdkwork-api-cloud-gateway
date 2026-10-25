# sdkwork-api-cloud-gateway Docker 安装与部署指南

> 规范依据：`sdkwork-specs/DOCKER_SPEC.md`、`sdkwork-specs/MODULE_BIN_SPEC.md`、`sdkwork-specs/DEPLOYMENT_SPEC.md` §6/§6.1、`sdkwork-specs/APPLICATION_GATEWAY_SPEC.md`。
> 本文档为唯一权威 Docker 安装文档，取代并已合并：`DOCKER_STANDALONE_QUICK_DEPLOY.md`、`REMOTE_DEPLOY_UBUNTU_DOCKER.md`、`INSTALLATION_STANDARDS.md`。English version: [docker-install.en.md](./docker-install.en.md)

**角色边界：** 网关是平台 API 网关，部署为**独立** Docker 组合，**永不公网直出**、不安装 nginx 站点映射、不启动 webserver；公网边缘由 `sdkwork-webserver` 反代触达（`DOCKER_SPEC.md` §1）。容器仅发布健康/代理端口 `GATEWAY_HOST_PORT` → 容器 `:3900`。

---

## 0. 快速上手（TL;DR）

```bash
bin/docker-image.sh build --image-tag <version>        # → registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:<version>
bin/docker-deploy.sh install --environment demo        # 本机 WSL（外置 PostgreSQL/Redis 为默认）
bin/docker-deploy.sh install --environment demo --host ssh://ops@10.0.0.8   # 远程 Ubuntu
```

离线安装（构建机不可达目标机时，仍从仓库侧执行 bin/）：

```bash
bin/docker-image.sh save -o dist/image.tar.gz              # 产出镜像 tar.gz + sha256
bin/docker-deploy.sh install --environment <environment> --host ssh://[user@]host [--replicas N]
```

bin/ 会完成 bundle 同步、sha256 校验、镜像加载与健康门禁；bundle 内执行器由 bin/ 调用，无需手动执行任何脚本。

## 1. 前置条件

| 项 | 要求 |
| --- | --- |
| Docker Engine + compose 插件 | 必须 |
| 宿主 PostgreSQL | `host.docker.internal:5432`（按环境建库 `sdkwork_ai_dev/test/staging/demo/prod`） |
| 宿主 Redis | `host.docker.internal:6379`（无密码；按环境 key 前缀 `cloudrouter-<env>`） |
| webserver | 公网接入由 `sdkwork-webserver` 提供（独立部署，先于或随后部署均可） |

## 2. 五环境矩阵

| 环境 | 主机端口键（默认） | profile id | 域名族（ALLOWED_HOSTS / CORS） | 数据库 |
| --- | --- | --- | --- | --- |
| development | `GATEWAY_DEV_HOST_PORT`（3910） | `standalone.development` | `api-dev.<base-domain>` 全家族 | `sdkwork_ai_dev` |
| test | 3911 | `standalone.test` | `api-test.*` | `sdkwork_ai_test` |
| staging | 3912 | `standalone.staging` | `api-staging.*` | `sdkwork_ai_staging` |
| demo | 3914 | `standalone.demo` | `api-demo.*` | `sdkwork_ai_demo` |
| production | 3913 | `standalone.production` | `api.*` | `sdkwork_ai_prod` |

- 容器内部统一 `:3900`；多实例端口步进 `GATEWAY_HOST_PORT + (index-1)*10`。
- `GATEWAY_ALLOWED_HOSTS` 必须枚举全部注册 base-domain 家族（`ENVIRONMENT_SPEC.md` §5.1.0.1）。
- 浏览器 CORS 分两半：`GATEWAY_CORS_ALLOWED_ORIGINS` 只放模式无法表达的精确源站（各环境环回端口），控制台闭包由**注册控制台主机模式**表达——`GATEWAY_CORS_CONSOLE_HOST_LABELS`（标签注册表，全平台 111 个）+ `GATEWAY_CORS_CONSOLE_HOST_SUFFIX`（`-dev`/`-test`/`-staging`/`-demo`/生产空）+ `GATEWAY_CORS_CONSOLE_HOST_SCHEMES`（非生产 `http,https`，生产 `https`）。模式按组件精确匹配，不会退化为任意子域通配；`app://*`/`tauri://*`/小程序源站由框架自动合并，不要写进 env。切勿再把控制台源站逐条枚举进 `*_CORS_ALLOWED_ORIGINS`（全量约 3000 条 / 100 KiB，逼近 Linux 128 KiB 单变量上限）。
- env 文件权威值：`docker/env/<environment>.env`（自 `*.env.example` 复制并填密钥）。改动后运行 `node scripts/derive-cors-allowed-origins.mjs --apply` 物化，`pnpm check:cors-carriers` 是门禁。

## 3. 标准安装流程（bin/ 入口）

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment <env> [--replicas N] [--host wsl|ssh://[user@]host[:port]]
bin/docker-deploy.sh upgrade  --environment <env> [--image-tag <new>]
bin/docker-deploy.sh rollback --environment <env>
bin/docker-deploy.sh status   --environment <env>
bin/docker-deploy.sh down     --environment <env> [--purge]   # --purge 需 --yes
```

- `install` 同步本模块 bundle（env/ + compose + 内置执行器）到目标主机 `/opt/deploy/sdkwork-api-cloud-gateway/bundle`，并在目标机侧由 bin/ 驱动完成滚动 apply；幂等。
- 本 bundle 带发布台账（release ledger），`rollback [--to <version>]` 依据台账回退。
- 多实例：实例 1 先行完成迁移并健康后，实例 2..N 才启动（`DOCKER_SPEC.md` §4.2 契约）。
- knowledgebase-rpc 单例在实例 1 之前由 `<project>-deps` 项目供应共享密钥材料。
- **§4 给出五个环境逐条展开、可直接复制的命令。**

## 4. 按环境命令速查（可直接复制）

> 以下每一块都可以整段复制执行。`<version>` 用实际版本号替换（当前
> `agent.app.config.json` → `release.currentVersion`，**省略 `--image-tag`
> 时自动取该值**）；`ops@10.0.0.8` 换成你的远程 Ubuntu 主机；`--host` 省略时
> 默认为 `wsl`（本机 WSL）。
> 全部变更类命令都可先加 `--dry-run` 打印计划而不执行。
> 本模块 bundle 带发布台账，因此 `rollback --to <version>` 有效。

### 4.1 development（开发）

端口 `3910` · profile `standalone.development` · 域名族 `api-dev.*` · 库 `sdkwork_ai_dev`

```bash
bin/docker-image.sh build --image-tag <version>                    # 全环境共用，已构建可跳过
bin/docker-deploy.sh install  --environment development            # 本机 WSL（幂等）
bin/docker-deploy.sh install  --environment development --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment development --image-tag <new-version>
bin/docker-deploy.sh rollback --environment development
bin/docker-deploy.sh rollback --environment development --to 0.1.0
bin/docker-deploy.sh status   --environment development
bin/docker-deploy.sh logs     --environment development
bin/docker-deploy.sh down     --environment development
bin/docker-deploy.sh stop    --environment development           # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment development           # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment development           # 只重启应用实例（依赖与网关不中断）
bin/docker-deploy.sh down     --environment development --purge --yes

curl --noproxy '*' http://127.0.0.1:3910/healthz
```

### 4.2 test（测试）

端口 `3911` · profile `standalone.test` · 域名族 `api-test.*` · 库 `sdkwork_ai_test`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment test
bin/docker-deploy.sh install  --environment test --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment test --image-tag <new-version>
bin/docker-deploy.sh rollback --environment test
bin/docker-deploy.sh rollback --environment test --to 0.1.0
bin/docker-deploy.sh status   --environment test
bin/docker-deploy.sh logs     --environment test
bin/docker-deploy.sh down     --environment test
bin/docker-deploy.sh stop    --environment test                  # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment test                  # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment test                  # 只重启应用实例（依赖与网关不中断）
bin/docker-deploy.sh down     --environment test --purge --yes

curl --noproxy '*' http://127.0.0.1:3911/healthz
```

### 4.3 staging（预发）

端口 `3912` · profile `standalone.staging` · 域名族 `api-staging.*` · 库 `sdkwork_ai_staging`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment staging
bin/docker-deploy.sh install  --environment staging --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment staging --image-tag <new-version>
bin/docker-deploy.sh rollback --environment staging
bin/docker-deploy.sh rollback --environment staging --to 0.1.0
bin/docker-deploy.sh status   --environment staging
bin/docker-deploy.sh logs     --environment staging
bin/docker-deploy.sh down     --environment staging
bin/docker-deploy.sh stop    --environment staging               # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment staging               # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment staging               # 只重启应用实例（依赖与网关不中断）
bin/docker-deploy.sh down     --environment staging --purge --yes

curl --noproxy '*' http://127.0.0.1:3912/healthz
```

### 4.4 demo（演示）

端口 `3914` · profile `standalone.demo` · 域名族 `api-demo.*` · 库 `sdkwork_ai_demo`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment demo
bin/docker-deploy.sh install  --environment demo --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment demo --image-tag <new-version>
bin/docker-deploy.sh rollback --environment demo
bin/docker-deploy.sh rollback --environment demo --to 0.1.0
bin/docker-deploy.sh status   --environment demo
bin/docker-deploy.sh logs     --environment demo
bin/docker-deploy.sh down     --environment demo
bin/docker-deploy.sh stop    --environment demo                  # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment demo                  # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment demo                  # 只重启应用实例（依赖与网关不中断）
bin/docker-deploy.sh down     --environment demo --purge --yes

curl --noproxy '*' http://127.0.0.1:3914/healthz
```

### 4.5 production（生产）

端口 `3913` · profile `standalone.production` · 域名族 `api.*` · 库 `sdkwork_ai_prod`

> **所有变更类命令必须带 `--yes`**，否则直接被拒绝（错误码 68）；`down --purge`
> 在**任何**环境都必须带 `--yes`。

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment production --yes
bin/docker-deploy.sh install  --environment production --yes --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment production --yes --image-tag <new-version>
bin/docker-deploy.sh rollback --environment production --yes
bin/docker-deploy.sh rollback --environment production --yes --to 0.1.0
bin/docker-deploy.sh status   --environment production             # 只读，不需要 --yes
bin/docker-deploy.sh logs     --environment production             # 只读，不需要 --yes
bin/docker-deploy.sh down     --environment production
bin/docker-deploy.sh stop    --environment production            # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment production            # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment production            # 只重启应用实例（依赖与网关不中断）
bin/docker-deploy.sh down     --environment production --purge --yes

curl --noproxy '*' http://127.0.0.1:3913/healthz
```

### 4.6 多实例与依赖模式

```bash
# 多实例：实例 1 先完成迁移并健康，实例 2..N 才启动（端口步进 +10）
bin/docker-deploy.sh install --environment demo --replicas 3

# 依赖模式：外置宿主 PostgreSQL/Redis 为默认；embedded 为显式 opt-in
bin/docker-deploy.sh install --environment demo --deps external     # 默认
bin/docker-deploy.sh install --environment demo --deps embedded
```

## 5. 验证

```bash
curl http://127.0.0.1:3910/healthz    # development（其余环境 3911/3912/3914/3913）
# 公网平面验证必须经 webserver（Host 探测）：
# curl --noproxy '*' -H 'Host: api-demo.sdkwork.com' http://<webserver-host>:19098/healthz
pnpm check:container-deployment 2>/dev/null || node scripts/validate-docker-deployment.mjs --help
```

**每个环境的端口与完整可复制命令见 §4.1–§4.5。**

### 5.1 配置预检与 env 键契约

```bash
# 只查不部署：fail-closed 校验全部必填键
bin/config.sh validate --environment production

# 查看当前配置（密钥自动脱敏）
bin/config.sh show --environment production

# 写入配置值（变更前自动备份并校验）
bin/config.sh set --environment production --key GATEWAY_POSTGRES_PASSWORD --value '<value>'

# 配置差异比对（部署态 vs bundle 基线）
bin/config.sh diff --environment production
```

**env 文件键契约（`env/<environment>.env`）**：
- 宿主侧键一律使用 `GATEWAY_` 前缀；compose 负责映射为容器侧 `SDKWORK_*` 名。compose 只插值它显式引用的变量——env 文件里 compose 未引用的键是死键，到不了容器。
- compose 模板从 env 文件消费的 `SDKWORK_*` 键**只有**数据库池参数（`SDKWORK_DATABASE_AUTO_MIGRATE` / `MAX_CONNECTIONS` / `MIN_CONNECTIONS` / `ACQUIRE_TIMEOUT` / `IDLE_TIMEOUT` / `MAX_LIFETIME` / `TEMPORARY_*`）以及可选的 `SDKWORK_DATABASE_URL` 凭据覆盖。除此之外不要在 env 文件里放任何 `SDKWORK_*` 键（容器环境变量来自镜像 ENV + compose environment 段）。
- 预检失败时收集**全部**缺口一次性报告，并给出傻瓜式修复指引（`--set` 或直接编辑 env 文件）。

## 6. 与 webserver 的对接

- webserver 通过 `imports.d/import.conf`（默认 cloud 集）把 `api-*` 平台域名反代到 gateway 上游（`sdkwork-api-cloud-gateway:8080` 或宿主端口）。
- 本组合**不配置**任何 webserver/nginx 内容；attach 模式由 webserver 侧 `SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK` 连入本网络。

## 6.1 运营生命周期（日志 / 配置 / 诊断 / 备份）

```bash
# 日志：默认有界读取，--follow 才持续跟踪
bin/docker-deploy.sh logs --environment <env> [--instance N] [--service gateway]
bin/docker-deploy.sh logs --environment <env> --tail 1000 --since 15m
bin/docker-deploy.sh logs --environment <env> --tail 5000 --export ./incident

# 配置：读取默认脱敏（***REDACTED***）；set/edit 先备份再做校验，生产需 --yes
bin/config.sh list     --environment <env>
bin/config.sh show     --environment <env>
bin/config.sh get      --environment <env> --key <KEY> [--reveal]
bin/config.sh set      --environment <env> --key <KEY> --value '<VALUE>'
bin/config.sh diff     --environment <env>      # 缺失键 / 未声明键 / 占位符
bin/config.sh validate --environment <env>
EDITOR=vi bin/config.sh edit --environment <env>

# 诊断：只读，9 项检查，存在 FAIL 时退出码 70
bin/doctor.sh --environment <env> [--json] [--export ./incident]

# 备份与恢复：集合在目标机 /opt/deploy/<module>/backups/
bin/backup.sh create  --environment <env> [--no-db] [--no-volumes]
bin/backup.sh list    --environment <env>
bin/backup.sh verify  --environment <env> [--set <name>]
bin/backup.sh restore --environment <env> --set <name> --yes
```

逐环境完整命令与处置路径见 `docs/runbooks/`（deploy / log-reference /
troubleshooting / backup-restore，中英双语）。

## 7. 规范索引

| 主题 | 权威规范 |
| --- | --- |
| 镜像命名、bundle、五环境矩阵、端口 | `sdkwork-specs/DOCKER_SPEC.md` |
| `bin/` 五入口契约 | `sdkwork-specs/MODULE_BIN_SPEC.md` |
| 外部依赖标准 | `sdkwork-specs/DEPLOYMENT_SPEC.md` §6.1 |
| 网关应用契约 | `sdkwork-specs/APPLICATION_GATEWAY_SPEC.md` |

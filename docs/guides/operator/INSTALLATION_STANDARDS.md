# SDKWork API Cloud Gateway — 安装与配置规范（多操作系统）

本文件是 `sdkwork-api-cloud-gateway` 的安装规范、配置文件规范与安装文件规范
（标准体系）的操作者文档。所有规范条款以 `sdkwork-specs/` 全局标准为准，本文件
只做应用级收窄与落地映射，不取代全局规范。

- 目录权威：`sdkwork-specs/RUNTIME_DIRECTORY_SPEC.md` §4.1–§4.5
- 环境/配置权威：`sdkwork-specs/ENVIRONMENT_SPEC.md` §3、§4、§8、§9、§11
- 打包权威：`sdkwork-specs/PACKAGING_SPEC.md` §3、§5.2、§5.5
- 部署权威：`sdkwork-specs/DEPLOYMENT_SPEC.md` §5.1、§5.4、§5.5
- 应用身份：`sdkwork.app.config.json`（appId `sdkwork-api-cloud-gateway`）

---

## 1. 应用身份与总则

| 项 | 值 | 来源 |
| --- | --- | --- |
| Application id | `sdkwork-api-cloud-gateway` | `sdkwork.app.config.json` |
| 应用码（application-code） | `api-gateway` | `specs/component.spec.json` capability，`SDKWORK_DEPLOY_SPEC.md` §3 |
| 进程名（process name） | `sdkwork-api-cloud-gateway` | 二进制名 |
| 配置格式 | **TOML** | `ENVIRONMENT_SPEC.md` §2/§9：“TOML is preferred for SDKWork Rust services” |
| 默认端口 | `3900`（应用公共入口） | `APP_RUNTIME_TOPOLOGY_SPEC.md` |

### 1.1 配置优先级（`ENVIRONMENT_SPEC.md` §3）

1. 内置安全默认值（本地开发/测试/桌面非机密设置）
2. 运行时配置文件（规范目录下的 TOML 或显式路径）
3. **进程环境变量**（覆盖配置文件，生产部署的注入面）
4. 命令行参数（仅本地开发与一次性工具，禁止用于生产服务管理器）
5. 密钥管理器 / OS 安全存储（平台可用时）

### 1.2 环境变量前缀（`ENVIRONMENT_SPEC.md` §4）

- 应用私有运行值前缀：`SDKWORK_<APPLICATION_CODE>_<SETTING>`，大写蛇形。
  本应用配置目录（`crates/sdkwork-api-cloud-gateway-config`）解析的
  `SDKWORK_API_CLOUD_GATEWAY_*` 系列按 legacy 前缀保留并文档化迁移方向：
  应用码迁移目标为 `SDKWORK_API_GATEWAY_*`，在配置 crate 切换前不强制改键。
- 共享键：`SDKWORK_CORS_ALLOWED_ORIGINS`、`SDKWORK_DATABASE_*`（统一工作区
  PostgreSQL 配置键）。
- 布尔值只接受 `true` / `false` / `1` / `0`；列表使用逗号分隔（§4）。

---

## 2. 目录规范速查（`RUNTIME_DIRECTORY_SPEC.md` §4.1–§4.5）

### 2.1 Linux System Scope（§4.1，Ubuntu/Debian 服务部署）

| 用途 | 规范路径 | 属主 |
| --- | --- | --- |
| 运行时配置 | `/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml` | `root:sdkwork` |
| 进程环境覆盖 | `/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.env` | `root:sdkwork`，0640 |
| 密钥文件 | `/etc/sdkwork/api-gateway/*.secret` | `root:sdkwork`，0600/0640 |
| 工作区数据库配置 | `/etc/sdkwork/database/database.toml` + `database.secret` | `root:sdkwork` |
| 私有不可变资产 | `/usr/lib/sdkwork/api-gateway`（或归档安装 `/opt/sdkwork/api-gateway`） | `root:root` |
| 共享只读资产 | `/usr/share/sdkwork/api-gateway` | `root:root` |
| 文档 | `/usr/share/doc/sdkwork/api-gateway` | `root:root` |
| 可变数据 | `/var/lib/sdkwork/api-gateway` | `sdkwork:sdkwork` |
| 日志 | `/var/log/sdkwork/api-gateway` | `sdkwork:adm` 或 `sdkwork:sdkwork` |
| 缓存 | `/var/cache/sdkwork/api-gateway` | `sdkwork:sdkwork` |
| 运行态 | `/run/sdkwork/api-gateway` | `sdkwork:sdkwork` |
| 服务注册 | `/usr/lib/systemd/system/sdkwork-api-gateway.service` | `PACKAGING_SPEC.md` §5.5 |

禁止裸进程名路径（如 `/etc/sdkwork-api-cloud-gateway`、`/var/lib/api-gateway`）。

### 2.2 Windows System Scope（§4.4，SCM 服务部署）

| 用途 | 规范路径 |
| --- | --- |
| 只读程序文件 | `%ProgramFiles%\sdkwork\api-gateway` |
| 配置根 | `%ProgramData%\sdkwork\api-gateway` |
| 配置文件 | `%ProgramData%\sdkwork\api-gateway\sdkwork-api-cloud-gateway.toml` |
| 数据 | `%ProgramData%\sdkwork\api-gateway\Data` |
| 日志 | `%ProgramData%\sdkwork\api-gateway\Logs` |
| 缓存 | `%ProgramData%\sdkwork\api-gateway\Cache` |
| 运行态 | `%ProgramData%\sdkwork\api-gateway\Run` |
| 密钥 | `%ProgramData%\sdkwork\api-gateway\Secrets` |
| 服务注册 | Windows 服务控制管理器，服务名 `sdkwork-api-gateway`（`PACKAGING_SPEC.md` §5.5） |

规则（§4.4 L209-211）：服务部署时 `%ProgramData%\sdkwork\api-gateway` 放可变/
可配置文件，`%ProgramFiles%\sdkwork\api-gateway` 放只读已安装二进制。

### 2.3 macOS System Scope（§4.3，launchd 部署）

| 用途 | 规范路径 |
| --- | --- |
| 配置 | `/Library/Application Support/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml` |
| 数据 | `/Library/Application Support/sdkwork/api-gateway/Data` |
| 日志 | `/Library/Logs/sdkwork/api-gateway` |
| 缓存 | `/Library/Caches/sdkwork/api-gateway` |
| 密钥 | `/Library/Application Support/sdkwork/api-gateway/Secrets` |
| 应用包 | `/Applications/SDKWork API Gateway.app`（如提供） |
| 服务注册 | `/Library/LaunchDaemons/sdkwork.api-gateway.plist`（§4.3 L177-182） |

### 2.4 Container Scope（§4.5，Docker/编排部署）

| 用途 | 规范路径 | 说明 |
| --- | --- | --- |
| 配置挂载 | `/etc/sdkwork/api-gateway` | 宿主 config 只读挂载 |
| 配置文件 | `/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml` | `SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE` 可覆盖 |
| 密钥挂载 | `/run/secrets/sdkwork/api-gateway` | 平台 secret 挂载（§11） |
| 数据卷 | `/var/lib/sdkwork/api-gateway` | 持久数据，禁止放镜像层 |
| 缓存卷 | `/var/cache/sdkwork/api-gateway` | 可重建 |
| 运行态 | `/run/sdkwork/api-gateway` | PID/锁 |
| 安装根 | `/opt/sdkwork/api-gateway` | 不可变包树（镜像层） |

容器内目录布局在 `Dockerfile` 中声明（§4.5 注释 + mkdir/chown），本仓库的
`docker-compose.yml` 按上表挂载 config 与数据卷。参考交付：`docker/`、
`docs/guides/operator/DOCKER_STANDALONE_QUICK_DEPLOY.md`。

---

## 3. Linux（Ubuntu）安装规范

### 3.1 安装方式

1. **归档安装**（当前交付，`TAR_GZ`）：解压到 `/opt/sdkwork/api-gateway`，随后
   手工完成 §2.1 目录/配置/服务注册（详见 `deployments/systemd/README.md`）。
2. **原生安装器**（规划，`.deb`）：按 `PACKAGING_SPEC.md` §5.5 矩阵落位：
   `/usr/lib/systemd/system/sdkwork-api-gateway.service`、
   `/etc/sdkwork/api-gateway/`、`/var/lib/sdkwork/api-gateway/` 等；
   `postinst`/`prerm` 只做运行时布局与服务注册（§5.2），禁止内嵌密钥。

### 3.2 服务单元（`deployments/systemd/sdkwork-api-gateway.service`）

```ini
[Service]
Type=simple
User=sdkwork
Group=sdkwork
EnvironmentFile=/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.env
WorkingDirectory=/opt/sdkwork/api-gateway
ExecStartPre=/opt/sdkwork/api-gateway/bin/sdkwork-api-cloud-gateway --migrate-databases
ExecStart=/opt/sdkwork/api-gateway/bin/sdkwork-api-cloud-gateway
```

- 安装期生成的环境赋值放 `EnvironmentFile`（0640），**不得内嵌到单元文件**
  （`PACKAGING_SPEC.md` §5.5）。
- 启动前完成：TOML 落位、`sdkwork` 系统用户、目录属主（§2.1）、
  PostgreSQL 就绪（密码走 `password_file` 或 `/etc/sdkwork/database/database.secret`）。

### 3.3 nginx（`NGINX_SPEC.md`）

站点文件：`/etc/nginx/sites-enabled/sdkwork/<domain>.conf`；上游指向
`http://127.0.0.1:3900`（容器部署使用环境端口 `3910` 至 `3913`）。仓库提供四个
站点模板：

| 模板 | 域名 | 说明 |
| --- | --- | --- |
| `docker/nginx/api-dev.sdkwork.com.conf` | `api-dev.*` 三开发域名 | HTTP 80，代理到 3910 |
| `docker/nginx/api-test.sdkwork.com.conf` | `api-test.*` 三测试域名 | HTTP 80，代理到 3911 |
| `docker/nginx/api-staging.sdkwork.com.conf` | `api-staging.*` 三预发布域名 | HTTP 80，代理到 3912 |
| `docker/nginx/api.sdkwork.com.conf` | 三个生产域名 | HTTPS 443（TLS 终止于 nginx）+ 80→443 重定向 |

**生产域名**：`api.sdkwork.com`、`api.birdcoder.com`、`api.dtupay.com`。
生产流量要求：

1. 三个域名真实 DNS 指向本机；为每个名称签发 TLS 证书（或 SAN/泛域名证书），
   替换 `api.sdkwork.com.conf` 中的证书路径
   （`/etc/ssl/sdkwork/api-gateway/fullchain.pem`、`privkey.pem`）。
2. 网关 Host 白名单与 CORS 必须包含三个生产域名——生产 TOML 模板
   （`etc/sdkwork-api-cloud-gateway.production.toml`）与容器挂载默认
   （`docker/config/sdkwork-api-cloud-gateway.toml`）的
   `[security].allowedHosts` 与 `[cors].allowedOrigins` 已内置
   `api.sdkwork.com` / `api.birdcoder.com` / `api.dtupay.com`；容器部署时再经
   `GATEWAY_ALLOWED_HOSTS` / `GATEWAY_CORS_ALLOWED_ORIGINS`（`.env.example`
   有生产示例）确认。
3. 主应用 origin 声明在 `etc/sdkwork.deployment.config.json` 的
   `production.applicationOrigin`（`https://api.sdkwork.com`）；其余两个域名是
   同一网关的公共入口别名，不需要额外声明。

---

## 4. Windows 安装规范（SCM 服务）

- 二进制：`%ProgramFiles%\sdkwork\api-gateway\bin\sdkwork-api-cloud-gateway.exe`
  （只读）
- 配置：`%ProgramData%\sdkwork\api-gateway\sdkwork-api-cloud-gateway.toml` +
  `%ProgramData%\sdkwork\api-gateway\sdkwork-api-cloud-gateway.env`
- 服务名：`sdkwork-api-gateway`（`PACKAGING_SPEC.md` §5.5），示例注册：

  ```powershell
  New-Service -Name sdkwork-api-gateway -BinaryPathName `
    "C:\Program Files\sdkwork\api-gateway\bin\sdkwork-api-cloud-gateway.exe" `
    -StartupType Automatic -Description "SDKWork API Cloud Gateway"
  $env:SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE = "C:\ProgramData\sdkwork\api-gateway\sdkwork-api-cloud-gateway.toml"
  ```

- 服务账户对 `%ProgramData%\sdkwork\api-gateway` 有读写、对
  `%ProgramFiles%\sdkwork\api-gateway` 只读。
- 原生安装器（`.msi`/`.exe`，规划）按 `PACKAGING_SPEC.md` §5.5 矩阵执行同样映射。

---

## 5. macOS 安装规范（launchd）

- 配置：`/Library/Application Support/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml`
- 服务注册：`/Library/LaunchDaemons/sdkwork.api-gateway.plist`（§4.3），关键键：

  ```xml
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/sdkwork-api-cloud-gateway</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE</key>
    <string>/Library/Application Support/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml</string>
  </dict>
  <key>UserName</key><string>_sdkwork</string>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  ```

- 原生安装器（`.pkg`/`.dmg`，规划）按 `PACKAGING_SPEC.md` §5.5 矩阵执行同样映射。

---

## 6. 配置规范（TOML + 环境变量分层）

### 6.1 TOML 结构（`ENVIRONMENT_SPEC.md` §9）

配置文件使用小写蛇形键，标准节：

| 节 | 内容 | 由 env 覆盖的关键项 |
| --- | --- | --- |
| `[service]` | name / environment / deployment_profile / profile_id / runtime_target | —（由部署姿态固定） |
| `[server]` | bind | `SDKWORK_API_CLOUD_GATEWAY_BIND` |
| `[cors]` | allowedOrigins | `SDKWORK_CORS_ALLOWED_ORIGINS`（共享键） |
| `[security]` | allowedHosts、trustedProxyCidrs、WAF、CSRF | `SDKWORK_API_CLOUD_GATEWAY_ALLOWED_HOSTS` |
| `[traffic]` | 限流/并发预算 | — |
| `[policy]` | 请求体/方法策略 | — |
| `[diagnostics]` | 诊断面 | — |
| `[observability]` | 日志/指标/追踪/OTLP | `RUST_LOG` |

生产模板：`etc/sdkwork-api-cloud-gateway.production.toml`；容器宿主可编辑副本：
`docker/config/sdkwork-api-cloud-gateway.toml`（挂载到 `/etc/sdkwork/api-gateway/`）。

### 6.2 数据库（`ENVIRONMENT_SPEC.md` §7.3、§9 `[database]`）

- 结构化 `SDKWORK_DATABASE_*` 字段优先（engine/host/port/name/schema/username/
  password/ssl_mode），密码材料用 `password_file` 或平台密钥，`url` 仅作私有
  operator 覆盖。
- 工作区共享配置：`/etc/sdkwork/database/database.toml` +
  `/etc/sdkwork/database/database.secret`（容器：`/etc/sdkwork/database/` 只读挂载）。
- 本应用规范化数据库标识：`sdkwork_ai_prod`（name == schema）。

### 6.3 容器部署注入面（docker compose / K8s）

部署期配置全部通过 env 注入（优先级高于挂载 TOML），见 `docker-compose.yml`：
ALLOWED_HOSTS、CORS、数据库、IM 目录、Cloud Router 密钥、Redis、对象存储根、
snowflake node id、支付主密钥文件路径。密钥不落入镜像层与 compose 文件本体。

---

## 7. 密钥规范（`RUNTIME_DIRECTORY_SPEC.md` §11 Secrets Standard）

- Linux：`/etc/sdkwork/api-gateway/*.secret`，`0600`/`0640`，仅服务账户与管理
  员可读；启动时缺失必需密钥必须 fail-closed（占位符安装器只用于安装期）。
- 容器：优先平台 secret 挂载 `/run/secrets/sdkwork/api-gateway/<secret-name>`
  （secret-name 即文件名），配置中引用路径或 `SDKWORK_*_FILE` env 指针。
- 数据库密码：`/etc/sdkwork/database/database.secret`（`password_file`），
  禁止写入 env 文件、单元文件、配置模板（`PACKAGING_SPEC.md` §5.5）。
- 禁止把生产密钥烘焙进镜像层（§4.5）。

---

## 8. 安装文件标准体系（`PACKAGING_SPEC.md` §3、`DEPLOYMENT_SPEC.md` §5.4）

服务包与容器构建上下文必须携带以下文件（由 `scripts/package-server.mjs` /
`scripts/build-api-cloud-gateway-container.mjs` 生成，全部落在 `dist/`）：

| 文件 | 作用 | 规范依据 |
| --- | --- | --- |
| `bin/sdkwork-api-cloud-gateway` | 可执行二进制 | §3 |
| `config/sdkwork-api-cloud-gateway.{development,test,production}.toml` | 配置模板（打包默认） | §3 / §5.4 |
| `INSTALL.md` | 安装指引（含本文件链接） | §5.4 |
| `install-manifest.json` | 清单：包身份/平台/配置模板/数据库模块/`installConfiguration` 目录映射/服务单元名 | §3、§5.5 |
| `checksums.sha256` | 完整性校验 | §3 |
| `specs/`（component.spec.json 等） | 组件契约与依赖面 | §3 |
| `sdkwork.app.config.json` | 应用身份与发布声明 | `APP_MANIFEST_SPEC.md` |
| `database-modules/<workspace>/database/` | 打包的数据库模块（含 `database.manifest.json`） | §3 |
| `container/`（容器目标） | entrypoint + metadata | §5.4 |

目录映射在 `install-manifest.json` 的 `installConfiguration` 中声明（§2 各表），
随包发布并链接 `RUNTIME_DIRECTORY_SPEC.md` 章节。

---

## 9. 验收清单

- [ ] 配置文件为 TOML，位于规范路径（§2 对应 OS 行），权限 0640/0644 合规
- [ ] 密钥材料在 `*.secret` / `/run/secrets/...` / `password_file`，不在配置与单元文件
- [ ] systemd 单元名 `sdkwork-api-gateway`，`EnvironmentFile` 0640，无内嵌 env 赋值
- [ ] 容器：config 挂载 `/etc/sdkwork/api-gateway/`、secret 挂载
      `/run/secrets/sdkwork/api-gateway`、数据卷 `/var/lib/sdkwork/api-gateway`
- [ ] 安装包含 `INSTALL.md`、`install-manifest.json`、`checksums.sha256`、
      配置模板、`specs/`，且 manifest 声明 `installConfiguration`
- [ ] `/healthz`、`/readyz` 通过；数据库经 `SDKWORK_DATABASE_*` 注入，
      名称 `sdkwork_ai_prod`

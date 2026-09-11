# SDKWork API Cloud Gateway 源码发布与部署指南

状态：active  
适用版本：`sdkwork-api-cloud-gateway 0.1.x`  
更新日期：2026-07-21  
规范：`DOCUMENTATION_SPEC.md`、`DEPLOYMENT_SPEC.md`、`RELEASE_SPEC.md`、`SUPPLY_CHAIN_SECURITY_SPEC.md`

本文给出一条可以重复执行的完整路径：准备 SDKWork 源码工作区，校验源码，分别生成并校验
`standalone` 和 `cloud` 服务器安装包，然后在 Ubuntu 或 Windows 上配置 PostgreSQL、安装为
系统服务、执行健康检查，并完成升级或回滚。

## 1. 部署结果与关键约束

完成后将得到以下运行拓扑：

```text
客户端 -> HTTPS 反向代理/负载均衡 -> 127.0.0.1:3900 -> Gateway 内嵌 API 组件 -> PostgreSQL
```

必须先理解以下约束：

1. 本仓库的 `Cargo.toml` 使用 `../sdkwork-*` 路径依赖。源码构建机必须具有完整的 SDKWork
   同级源码工作区，只下载 `sdkwork-api-cloud-gateway` 一个仓库无法构建完整
   `platform-foundation-suite`。
2. Ubuntu 产物必须在 Ubuntu/Linux 构建，Windows 产物必须在 Windows 构建。不要把一种平台
   上的原生二进制改名后发布到另一平台。
3. 官方本地发布入口是 `pnpm gateway:package:standalone` 和 `pnpm gateway:package:cloud`。它们会
   以 release 模式编译全部基础组件、收集数据库模块、生成 SHA-256 校验清单，并在
   `dist/server/` 生成各自的 `tar.gz` 安装包。
4. 服务进程的当前工作目录必须是安装包根目录。程序从 `.` 查找
   `sdkwork.app.config.json` 和 `specs/component.spec.json`，相对配置路径也以该目录解析。
5. `etc/` 和安装包中的 `config/` 只是安全模板。数据库密码、TLS 私钥和其他秘密必须由运维环境
   单独注入，不能写回 Git、安装包或日志。
6. 网关监听 HTTP。生产 TLS 应在 Nginx、IIS、云负载均衡或其他受管入口终止，只向外开放
   `80/443`，不要直接把 `3900` 暴露到互联网。

### 1.1 选择部署档案

| 档案 | 适用场景 | 包标识 | 生产 profile id |
| --- | --- | --- | --- |
| `standalone` | 单机、边缘节点、单个虚拟机或由运维直接管理的服务 | `*-standalone-server.tar.gz` | `standalone.production` |
| `cloud` | 云平台、多副本、负载均衡或编排平台管理的网关实例 | `*-cloud-server.tar.gz` | `cloud.production` |

两个档案构建相同的 `platform-foundation-suite` 内嵌 API 组件，并使用同一个 production TOML
模板。`cloud` 不是把请求转发到远程基础 API 的精简代理；它仍然启动完整网关进程。二者的差异是
发布包身份和部署拓扑元数据。每次安装只能选择一个档案，环境中的
`SDKWORK_DEPLOYMENT_PROFILE`、`SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE` 和
`SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID` 必须与所选包一致。

以下命令默认 SDKWork 工作区位于：

- Ubuntu：`/srv/sdkwork-space`
- Windows：`C:\sdkwork-space`

可以使用其他目录，但所有 `sdkwork-*` 仓库必须保持同级关系。

## 2. 发布前准备

### 2.1 源码目录

构建机至少需要当前 `Cargo.toml` 和打包脚本引用的所有同级仓库。典型结构如下：

```text
sdkwork-space/
  sdkwork-api-cloud-gateway/
  sdkwork-specs/
  sdkwork-github-workflow/
  sdkwork-database/
  sdkwork-utils/
  sdkwork-web-framework/
  sdkwork-iam/
  sdkwork-drive/
  ...Cargo.toml 中其余 sdkwork-* 路径依赖...
```

所有仓库应检出同一发布基线。发布前在每个仓库记录提交 SHA；不要混用互不兼容的分支。

### 2.2 工具版本

两种平台均需要：

- Git
- Rust stable、Cargo 和 `rustfmt`、`clippy`
- Node.js LTS
- Corepack，以及仓库声明的 `pnpm 10.33.0`
- PostgreSQL 服务器或一个可访问的受管 PostgreSQL 实例
- `tar`；现代 Ubuntu 和 Windows 10/11 通常已经提供

在仓库根目录确认工具：

```text
git --version
rustc --version
cargo --version
node --version
corepack --version
tar --version
```

启用仓库指定的 pnpm：

```text
corepack enable
corepack prepare pnpm@10.33.0 --activate
pnpm --version
```

## 3. Ubuntu 构建发布包

### 3.1 安装构建依赖

以下示例适用于 Ubuntu 22.04/24.04：

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev curl git tar postgresql-client
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup toolchain install stable
rustup default stable
rustup component add rustfmt clippy
```

按 Node.js 官方支持方式安装当前 LTS 后，执行第 2.2 节的 Corepack 命令。

### 3.2 校验源码

```bash
cd /srv/sdkwork-space/sdkwork-api-cloud-gateway
cargo metadata --no-deps --format-version 1
cargo fmt -- --check
pnpm check
pnpm workflow:validate
pnpm gateway:matrix:standalone
pnpm gateway:matrix:cloud
```

任何命令失败都应先修复源码或工作区版本，不要继续发布。

### 3.3 构建、测试并打包

完整合并级校验可能耗时较长，但正式发布应执行：

```bash
pnpm verify
pnpm gateway:package:standalone
pnpm gateway:validate:standalone
pnpm gateway:package:cloud
pnpm gateway:validate:cloud
```

打包脚本自动设置 `CARGO_INCREMENTAL=0`，实际编译命令等价于：

```bash
cargo build --release \
  -p sdkwork-api-cloud-gateway \
  --bin sdkwork-api-cloud-gateway \
  --no-default-features \
  --features platform-foundation-suite
```

查看产物：

```bash
ls -lh dist/server/*.tar.gz
tar -tzf dist/server/*.tar.gz | head -40
```

默认 x64 文件名分别为：

```text
dist/server/sdkwork-api-cloud-gateway-0.1.0-linux-x64-standalone-server.tar.gz
dist/server/sdkwork-api-cloud-gateway-0.1.0-linux-x64-cloud-server.tar.gz
```

只发布一个档案时可以只执行对应的一组 `package` 和 `validate`；正式发布两个档案时，两组验证
都必须成功。后续 Ubuntu 示例使用 `/tmp/gateway.tar.gz` 代表所选档案的包。

## 4. Ubuntu 生产部署

### 4.1 创建账号和目录

```bash
sudo useradd --system --home /opt/sdkwork-api-cloud-gateway --shell /usr/sbin/nologin sdkwork-gateway
sudo mkdir -p /opt/sdkwork-api-cloud-gateway/releases /etc/sdkwork-api-cloud-gateway
sudo chown -R sdkwork-gateway:sdkwork-gateway /opt/sdkwork-api-cloud-gateway
sudo chmod 750 /etc/sdkwork-api-cloud-gateway
```

### 4.2 解压并固定版本

把本次 `tar.gz` 上传到服务器，例如 `/tmp/gateway.tar.gz`，然后执行：

```bash
sudo tar -xzf /tmp/gateway.tar.gz -C /opt/sdkwork-api-cloud-gateway/releases
RELEASE_DIR="$(tar -tzf /tmp/gateway.tar.gz | head -1 | cut -d/ -f1)"
sudo chown -R sdkwork-gateway:sdkwork-gateway "/opt/sdkwork-api-cloud-gateway/releases/$RELEASE_DIR"
sudo ln -sfn "/opt/sdkwork-api-cloud-gateway/releases/$RELEASE_DIR" /opt/sdkwork-api-cloud-gateway/current
```

校验安装包内文件。`sha256sum -c` 必须在发布目录内执行：

```bash
cd /opt/sdkwork-api-cloud-gateway/current
sha256sum -c checksums.sha256
```

### 4.3 创建运维配置

复制生产模板到受保护目录，不直接修改 release：

```bash
sudo cp /opt/sdkwork-api-cloud-gateway/current/config/sdkwork-api-cloud-gateway.production.toml \
  /etc/sdkwork-api-cloud-gateway/gateway.toml
sudo chown root:sdkwork-gateway /etc/sdkwork-api-cloud-gateway/gateway.toml
sudo chmod 640 /etc/sdkwork-api-cloud-gateway/gateway.toml
```

编辑 `/etc/sdkwork-api-cloud-gateway/gateway.toml`，至少修改：

- `[cors].allowedOrigins`：真实前端 HTTPS Origin，必须包含协议，不带路径。只放模式无法表达的精确源站（例如环回端口）；模块控制台改用 `[cors.registeredConsoleHosts]` 声明 `labels` / `environmentSuffix` / `schemes`（`baseDomains` 省略即用平台注册表，不允许放宽到未注册域名）。
- `[security].allowedHosts`：用户实际访问的域名；经代理转发时确保代理保留正确的 `Host`。
- `[security].trustedProxyCidrs`：只填写真实可信代理网段。
- `[security.waf].mode`：先以 `monitor` 观察，评审后再切到 `enforce`。
- `[server].bind`：使用本机代理时建议改为 `127.0.0.1:3900`；跨主机负载均衡时使用
  `0.0.0.0:3900` 并用安全组限制来源。

### 4.4 准备 PostgreSQL

生产环境使用 SDKWork workspace 统一的数据库、schema 和角色。下面的 SQL 仅演示首次供应；
密码必须由密码管理系统生成，同一 workspace 中的各应用和内嵌模块不得另建私有数据库或 schema：

```sql
CREATE ROLE sdkwork_ai_prod LOGIN PASSWORD '<CHANGE_ME_STRONG_PASSWORD>';
CREATE DATABASE sdkwork_ai_prod OWNER sdkwork_ai_prod;
\connect sdkwork_ai_prod
CREATE SCHEMA sdkwork_ai_prod AUTHORIZATION sdkwork_ai_prod;
```

创建 `/etc/sdkwork-api-cloud-gateway/gateway.env`：

```dotenv
RUST_LOG=sdkwork_api_cloud_gateway=info,tower_http=info
SDKWORK_ENVIRONMENT=production
SDKWORK_DEPLOYMENT_PROFILE=<standalone_or_cloud>
SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT=production
SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE=<standalone_or_cloud>
SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID=<standalone_or_cloud>.production
SDKWORK_API_CLOUD_GATEWAY_RUNTIME_TARGET=server
SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE=/etc/sdkwork-api-cloud-gateway/gateway.toml
SDKWORK_DATABASE_ENGINE=postgresql
SDKWORK_DATABASE_HOST=db.example.internal
SDKWORK_DATABASE_PORT=5432
SDKWORK_DATABASE_NAME=sdkwork_ai_prod
SDKWORK_DATABASE_SCHEMA=sdkwork_ai_prod
SDKWORK_DATABASE_USERNAME=sdkwork_ai_prod
SDKWORK_DATABASE_PASSWORD_FILE=/etc/sdkwork-api-cloud-gateway/database.secret
SDKWORK_DATABASE_SSL_MODE=require
SDKWORK_DATABASE_MAX_CONNECTIONS=20
PGOPTIONS=-csearch_path=sdkwork_ai_prod
SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL=http://127.0.0.1:3900
```

将数据库密码写入 `/etc/sdkwork-api-cloud-gateway/database.secret`，文件只包含密码本身，并限制为
服务账号可读。生产环境必须启用 PostgreSQL TLS；具体证书参数以数据库供应商要求为准。

所有内嵌模块复用上述 `SDKWORK_DATABASE_*` 连接身份和进程共享连接池。数据库配置键只能以
`SDKWORK_DATABASE_` 开头，不得增加服务或模块前缀。RTC 和 AIoT 的业务存储开关不是数据库连接身份，可按需配置：

```dotenv
SDKWORK_AIOT_DEVICE_STORAGE_TABLE_PREFIX=iot_
SDKWORK_RTC_STATE_REQUIRE_DURABLE=true
SDKWORK_RTC_REQUIRE_OUTBOX=true
```

不要在生产中照搬开发环境的 `SDKWORK_DATABASE_TEMPORARY_*` 例外开关。

保护环境文件：

```bash
sudo chown root:sdkwork-gateway /etc/sdkwork-api-cloud-gateway/database.secret
sudo chmod 640 /etc/sdkwork-api-cloud-gateway/database.secret
sudo chown root:sdkwork-gateway /etc/sdkwork-api-cloud-gateway/gateway.env
sudo chmod 640 /etc/sdkwork-api-cloud-gateway/gateway.env
```

### 4.5 安装 systemd 服务

创建 `/etc/systemd/system/sdkwork-api-cloud-gateway.service`。该 unit 同时适用于两个档案，实际
档案由 `gateway.env` 和 `/opt/sdkwork-api-cloud-gateway/current` 指向的安装包共同决定：

```ini
[Unit]
Description=SDKWork API Cloud Gateway
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=sdkwork-gateway
Group=sdkwork-gateway
WorkingDirectory=/opt/sdkwork-api-cloud-gateway/current
EnvironmentFile=/etc/sdkwork-api-cloud-gateway/gateway.env
ExecStart=/opt/sdkwork-api-cloud-gateway/current/bin/sdkwork-api-cloud-gateway --config /etc/sdkwork-api-cloud-gateway/gateway.toml
Restart=on-failure
RestartSec=5
TimeoutStopSec=30
KillSignal=SIGTERM
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/sdkwork-api-cloud-gateway
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

加载并启动：

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now sdkwork-api-cloud-gateway
sudo systemctl status sdkwork-api-cloud-gateway --no-pager
sudo journalctl -u sdkwork-api-cloud-gateway -n 200 --no-pager
```

### 4.6 健康检查

生产模板默认限制 `Host`，因此本机检查要发送允许的 Host：

```bash
curl --fail --show-error -H 'Host: api.example.com' http://127.0.0.1:3900/healthz
curl --fail --show-error -H 'Host: api.example.com' http://127.0.0.1:3900/readyz
curl --fail --show-error -H 'Host: api.example.com' http://127.0.0.1:3900/metrics | head
```

`/healthz` 表示进程可响应；只有 `/readyz` 成功才表示内嵌路由和依赖已完成启动。负载均衡应使用
`/readyz`，部署自动化必须在切流前等待它成功。

## 5. Windows 构建发布包

### 5.1 安装工具

在管理员 PowerShell 中使用组织批准的安装方式安装：

- Git for Windows
- Rustup（MSVC toolchain）
- Visual Studio 2022 Build Tools，勾选“使用 C++ 的桌面开发”和 Windows SDK
- Node.js LTS
- PostgreSQL client（需要本机排查数据库时）

打开新的 PowerShell：

```powershell
rustup toolchain install stable-x86_64-pc-windows-msvc
rustup default stable-x86_64-pc-windows-msvc
rustup component add rustfmt clippy
corepack enable
corepack prepare pnpm@10.33.0 --activate
```

### 5.2 校验、构建和打包

```powershell
Set-Location C:\sdkwork-space\sdkwork-api-cloud-gateway
cargo metadata --no-deps --format-version 1
cargo fmt -- --check
pnpm check
pnpm workflow:validate
pnpm gateway:matrix:standalone
pnpm gateway:matrix:cloud
pnpm verify
pnpm gateway:package:standalone
pnpm gateway:validate:standalone
pnpm gateway:package:cloud
pnpm gateway:validate:cloud
Get-ChildItem .\dist\server\*.tar.gz
```

打包脚本已设置 `CARGO_INCREMENTAL=0`，用于规避 Windows release 增量编译的已知 rustc 问题。
默认 x64 产物分别为：

```text
dist\server\sdkwork-api-cloud-gateway-0.1.0-windows-x64-standalone-server.tar.gz
dist\server\sdkwork-api-cloud-gateway-0.1.0-windows-x64-cloud-server.tar.gz
```

第 6 节选择其中一个包安装。示例使用 `standalone` 文件名；部署 `cloud` 时把归档名和
`$Release` 中的 `standalone` 替换为 `cloud`，并按下文设置 cloud profile 环境变量。

## 6. Windows 生产部署

### 6.1 解压并准备目录

在管理员 PowerShell 中执行，实际文件名按本次版本替换：

```powershell
New-Item -ItemType Directory -Force C:\ProgramData\SDKWork\ApiCloudGateway\releases | Out-Null
New-Item -ItemType Directory -Force C:\ProgramData\SDKWork\ApiCloudGateway\config | Out-Null
tar -xzf C:\Temp\sdkwork-api-cloud-gateway-0.1.0-windows-x64-standalone-server.tar.gz `
  -C C:\ProgramData\SDKWork\ApiCloudGateway\releases
$Release = 'C:\ProgramData\SDKWork\ApiCloudGateway\releases\sdkwork-api-cloud-gateway-0.1.0-windows-x64-standalone-server'
Set-Content -Path C:\ProgramData\SDKWork\ApiCloudGateway\current.txt -Value $Release
Copy-Item "$Release\config\sdkwork-api-cloud-gateway.production.toml" `
  C:\ProgramData\SDKWork\ApiCloudGateway\config\gateway.toml
```

用以下命令验证归档摘要；输出必须与安装包内 `checksums.sha256` 对应：

```powershell
Get-FileHash "$Release\bin\sdkwork-api-cloud-gateway.exe" -Algorithm SHA256
Get-Content "$Release\checksums.sha256"
```

编辑 `C:\ProgramData\SDKWork\ApiCloudGateway\config\gateway.toml`，修改项与 Ubuntu 第 4.3 节
一致。若反向代理也在本机，建议绑定 `127.0.0.1:3900`。

### 6.2 设置服务账号的环境变量

Windows 服务不读取 `.env`。建议通过组织的秘密管理工具为专用服务账号注入变量。小规模部署可
将变量设置为机器级环境变量，但这会使同机其他高权限进程可读取数据库凭据：

```powershell
[Environment]::SetEnvironmentVariable('SDKWORK_ENVIRONMENT', 'production', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_DEPLOYMENT_PROFILE', '<standalone_or_cloud>', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT', 'production', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE', '<standalone_or_cloud>', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID', '<standalone_or_cloud>.production', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_API_CLOUD_GATEWAY_RUNTIME_TARGET', 'server', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE', 'C:\ProgramData\SDKWork\ApiCloudGateway\config\gateway.toml', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_DATABASE_ENGINE', 'postgresql', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_DATABASE_HOST', 'db.example.internal', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_DATABASE_PORT', '5432', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_DATABASE_NAME', 'sdkwork_ai_prod', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_DATABASE_SCHEMA', 'sdkwork_ai_prod', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_DATABASE_USERNAME', 'sdkwork_ai_prod', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_DATABASE_PASSWORD_FILE', 'C:\ProgramData\SDKWork\ApiCloudGateway\config\database.secret', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_DATABASE_SSL_MODE', 'require', 'Machine')
[Environment]::SetEnvironmentVariable('SDKWORK_DATABASE_MAX_CONNECTIONS', '20', 'Machine')
[Environment]::SetEnvironmentVariable('PGOPTIONS', '-csearch_path=sdkwork_ai_prod', 'Machine')
[Environment]::SetEnvironmentVariable('RUST_LOG', 'sdkwork_api_cloud_gateway=info,tower_http=info', 'Machine')
```

秘密管理工具必须将密码写入 `database.secret`，并将文件 ACL 限制为服务账号和管理员可读。
所有内嵌模块共享这组数据库变量；修改机器环境变量后必须重启服务进程才能生效。

### 6.3 先进行前台冒烟测试

```powershell
Set-Location $Release
& .\bin\sdkwork-api-cloud-gateway.exe `
  --config C:\ProgramData\SDKWork\ApiCloudGateway\config\gateway.toml
```

在另一个 PowerShell 窗口检查，其中 `api.example.com` 必须已写入 `allowedHosts`：

```powershell
Invoke-RestMethod http://127.0.0.1:3900/healthz -Headers @{ Host = 'api.example.com' }
Invoke-RestMethod http://127.0.0.1:3900/readyz -Headers @{ Host = 'api.example.com' }
```

成功后按 `Ctrl+C` 停止前台进程。

### 6.4 使用 WinSW 安装 Windows 服务

WinSW 可提供自动启动、失败重启、日志滚动和优雅停止。下载组织审计通过的 WinSW 3 x64
可执行文件，校验官方摘要后，将其放到：

```text
C:\ProgramData\SDKWork\ApiCloudGateway\service\sdkwork-api-cloud-gateway-service.exe
```

在同目录创建 `sdkwork-api-cloud-gateway-service.xml`：

```xml
<service>
  <id>sdkwork-api-cloud-gateway</id>
  <name>SDKWork API Cloud Gateway</name>
  <description>SDKWork Rust/Axum platform API gateway</description>
  <executable>C:\ProgramData\SDKWork\ApiCloudGateway\releases\sdkwork-api-cloud-gateway-0.1.0-windows-x64-standalone-server\bin\sdkwork-api-cloud-gateway.exe</executable>
  <arguments>--config C:\ProgramData\SDKWork\ApiCloudGateway\config\gateway.toml</arguments>
  <workingdirectory>C:\ProgramData\SDKWork\ApiCloudGateway\releases\sdkwork-api-cloud-gateway-0.1.0-windows-x64-standalone-server</workingdirectory>
  <startmode>Automatic</startmode>
  <stoptimeout>30sec</stoptimeout>
  <onfailure action="restart" delay="5 sec" />
  <log mode="roll-by-size-time">
    <sizeThreshold>10240</sizeThreshold>
    <pattern>yyyyMMdd</pattern>
    <autoRollAtTime>00:00:00</autoRollAtTime>
    <zipOlderThanNumDays>7</zipOlderThanNumDays>
    <zipDateFormat>yyyyMM</zipDateFormat>
  </log>
</service>
```

安装、启动并检查：

```powershell
Set-Location C:\ProgramData\SDKWork\ApiCloudGateway\service
.\sdkwork-api-cloud-gateway-service.exe install
.\sdkwork-api-cloud-gateway-service.exe start
Get-Service sdkwork-api-cloud-gateway
Invoke-RestMethod http://127.0.0.1:3900/readyz -Headers @{ Host = 'api.example.com' }
```

限制 `C:\ProgramData\SDKWork\ApiCloudGateway` 的 ACL，只允许 Administrators、SYSTEM 和专用
服务账号读取配置；数据库凭据不应授予普通用户读取权限。Windows Defender Firewall 只允许
反向代理或负载均衡节点访问 `3900`。

## 7. 反向代理检查清单

无论使用 Nginx、IIS 还是云负载均衡，都应满足：

1. 对外只开放 HTTPS，TLS 证书由入口层管理。
2. 转发原始 `Host`，并设置 `X-Forwarded-For`、`X-Forwarded-Proto`；只把真实代理网段加入
   `trustedProxyCidrs`。
3. 上游连接到 `127.0.0.1:3900` 或受防火墙保护的内网地址。
4. 健康检查使用 `/readyz`，并发送 `allowedHosts` 接受的 Host。
5. 上传大小不超过 `[request].maxBodyBytes`，代理超时应覆盖业务最长合法请求。
6. WebSocket 场景要启用 HTTP/1.1 Upgrade 转发。

## 8. 验收步骤

部署完成后逐项确认：

```text
[ ] 安装包 SHA-256 校验通过
[ ] 服务使用受限账号运行，而不是 root/Administrator
[ ] 配置和秘密不在 Git、release 目录或日志中
[ ] PostgreSQL 使用 workspace 统一的 sdkwork_ai_prod 数据库/schema/账号和 TLS
[ ] /healthz 返回成功
[ ] /readyz 返回成功并显示预期依赖路由
[ ] /metrics 只通过受控网络访问
[ ] 公网 HTTPS 域名可访问，Host allowlist 和 CORS 符合预期
[ ] 3900 未直接暴露到公网
[ ] 重启操作系统后服务自动恢复
[ ] 日志已接入集中采集并设置保留策略
[ ] 已保留上一个可用 release 和对应配置备份
```

## 9. 升级与回滚

### 9.1 Ubuntu

升级时把新包解压到新的版本目录，校验摘要，停止服务，原子更新 `current` 链接，再启动并检查
`/readyz`：

```bash
sudo systemctl stop sdkwork-api-cloud-gateway
sudo ln -sfn /opt/sdkwork-api-cloud-gateway/releases/<NEW_RELEASE> /opt/sdkwork-api-cloud-gateway/current
sudo systemctl start sdkwork-api-cloud-gateway
curl --fail -H 'Host: api.example.com' http://127.0.0.1:3900/readyz
```

回滚时把链接切回旧 release 并重启。若新版本执行了不向后兼容的数据库迁移，必须使用该版本的
迁移方案，不能只回滚二进制。

### 9.2 Windows

停止 WinSW 服务，把 XML 中的 `<executable>` 和 `<workingdirectory>` 改为新版本目录，然后
刷新并启动：

```powershell
.\sdkwork-api-cloud-gateway-service.exe stop
.\sdkwork-api-cloud-gateway-service.exe refresh
.\sdkwork-api-cloud-gateway-service.exe start
```

健康检查失败时停止服务，把 XML 改回旧版本，`refresh` 后重新启动。不要覆盖旧版本目录，这样
回滚不依赖重新解压。

## 10. 常见故障

### Cargo 报 path dependency 不存在

原因是缺少同级 `sdkwork-*` 仓库或目录层级不正确。根据根 `Cargo.toml` 的
`[workspace.dependencies]` 补齐仓库，并确认所有仓库处于兼容提交。

### 启动时报 missing SDKWORK_DATABASE configuration

进程没有读到 `SDKWORK_DATABASE_URL` 或拆分的 `SDKWORK_DATABASE_*` 字段。检查 systemd
`EnvironmentFile`、Windows 服务账号环境和密码文件 ACL；不要把密码打印到工单或日志。

### `/healthz` 或 `/readyz` 返回 421

请求的 Host 不在 `[security].allowedHosts`。使用实际生产域名作为 Host，并确认反向代理没有把
Host 改写为 `127.0.0.1:3900`。

### `/healthz` 成功但 `/readyz` 失败

进程存活，但数据库初始化、内嵌组件或路由预检未完成。查看启动日志中第一个
`embedded database module` 或 `preflight` 错误；不要让负载均衡切流。

### Windows release 编译异常

使用仓库的打包命令；它强制 `CARGO_INCREMENTAL=0`。若手工构建，也先执行：

```powershell
$env:CARGO_INCREMENTAL = '0'
cargo build --release -p sdkwork-api-cloud-gateway --bin sdkwork-api-cloud-gateway `
  --no-default-features --features platform-foundation-suite
```

### 端口 3900 被占用

Ubuntu 使用 `sudo ss -ltnp | grep :3900`，Windows 使用
`Get-NetTCPConnection -LocalPort 3900` 找到占用进程。不要通过启动第二个网关实例掩盖旧服务。

## 11. 开发直跑（非生产）

本地开发使用仓库自带 PostgreSQL 初始化和环境展开逻辑。复制忽略的开发配置：

```text
.env.postgres.example -> .env.postgres
```

替换所有 `<CHANGE_ME>` 后，在仓库根运行：

```text
pnpm db:postgres:plan
pnpm dev
```

`pnpm dev` 会选择 `standalone.development`、初始化 PostgreSQL，并用
`platform-foundation-suite` 启动网关。该入口包含开发期临时数据库兼容变量，不应作为生产服务
管理器使用。

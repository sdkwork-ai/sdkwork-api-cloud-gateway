# systemd 部署（Ubuntu / Debian）

本目录提供 SDKWork API Cloud Gateway 的 systemd 服务单元模板：

- `sdkwork-api-gateway.service` — 服务单元（规范位置
  `/usr/lib/systemd/system/sdkwork-api-gateway.service`，服务名
  `sdkwork-api-gateway`，见 `PACKAGING_SPEC.md` §5.5 安装目标矩阵）
- `sdkwork-knowledgebase-rpc.service` — 网关随包安装的 Knowledgebase 内部 RPC
  sidecar；仅监听 `127.0.0.1:50054`，并通过持久化 mTLS 和签名调用上下文与
  网关中的 IM 模块通信。

安装路径遵循 `topology.applicationCode = api-gateway`
（`APPLICATION_DEPLOY_LAYOUT_SPEC` / `RUNTIME_DIRECTORY_SPEC` §4.1）：

| Purpose | Path |
| --- | --- |
| Config | `/etc/sdkwork/api-gateway/config.toml` |
| Process env | `/etc/sdkwork/api-gateway/api-gateway.env` |
| Secrets | `/etc/sdkwork/api-gateway/secrets/` |
| Data | `/var/lib/sdkwork/api-gateway` |
| Install | `/opt/sdkwork/api-gateway` |

与 Docker 容器布局一致（容器 secrets 在 `/run/secrets/sdkwork/api-gateway`）。

## 安装步骤（Ubuntu）

安装前先校验工作流发布的归档 sidecar 和 detached signature：

```bash
sha256sum -c sdkwork-api-cloud-gateway-<version>-linux-x64-standalone-server.tar.gz.sha256
openssl dgst -sha256 \
  -verify sdkwork-api-cloud-gateway-<version>-linux-x64-standalone-server.tar.gz.pub.pem \
  -signature sdkwork-api-cloud-gateway-<version>-linux-x64-standalone-server.tar.gz.sig \
  sdkwork-api-cloud-gateway-<version>-linux-x64-standalone-server.tar.gz
```

执行前必须把 `.pub.pem` 的指纹与受保护发布渠道中的 SDKWork release key 指纹核对；
仅验证同一下载目录中的公钥和签名不能建立发布者信任。

同一发布还必须携带 `.cdx.json` CycloneDX SBOM、`.intoto.jsonl` SLSA/in-toto
provenance 和 `.sdkwork/evidence/<package-id>.json`。artifact evidence 的 digest、版本、
source commit、package id、deployment profile 与 runtime target 必须与待安装归档一致。

推荐使用包内安装器（落位 `config.toml` + `api-gateway.env` + secrets）：

```bash
# 在解压后的服务包根目录执行（包内安装器落位于 install/）
sudo bash install/install-ubuntu.sh \
  --environment production \
  --deployment-profile standalone
```

> 安装器源码位于仓库 `bin/packaging/install-ubuntu.sh`（MODULE_BIN_SPEC.md §2.1），
> 由 `scripts/package-server.mjs` 打包进归档的 `install/` 目录。

手工步骤（与安装器等价）：

```bash
# 1. 解压服务包到安装根
sudo mkdir -p /opt/sdkwork/api-gateway
sudo tar -xzf sdkwork-api-cloud-gateway-<version>-linux-x64-standalone-server.tar.gz \
  -C /opt/sdkwork/api-gateway --strip-components=1

# 2. 创建系统用户与运行时目录
sudo groupadd --system sdkwork || true
sudo useradd --system --gid sdkwork --home-dir /opt/sdkwork/api-gateway sdkwork || true
sudo install -d -o root -g sdkwork -m 0750 /etc/sdkwork/api-gateway \
  /etc/sdkwork/api-gateway/secrets
sudo install -d -o sdkwork -g sdkwork -m 0750 /var/lib/sdkwork/api-gateway \
  /var/log/sdkwork/api-gateway /var/cache/sdkwork/api-gateway /run/sdkwork/api-gateway

# 3. 落位运行时配置（APPLICATION_DEPLOY_LAYOUT_SPEC: config.toml）
sudo install -o root -g sdkwork -m 0640 \
  config/sdkwork-api-cloud-gateway.production.toml \
  /etc/sdkwork/api-gateway/config.toml
sudo install -o root -g sdkwork -m 0640 /dev/null \
  /etc/sdkwork/api-gateway/api-gateway.env
sudo editor /etc/sdkwork/api-gateway/api-gateway.env
#   SDKWORK_DATABASE_* 连接注入（密码走 /etc/sdkwork/database/database.secret
#   或 SDKWORK_DATABASE_PASSWORD_FILE，禁止写入本文件，PACKAGING_SPEC §5.5）
#   Redis：外部实例、无密码（空 SDKWORK_CLOUDROUTER_REDIS_PASSWORD）

# 4. 安装服务单元并启动
sudo install -o root -g root -m 0644 \
  sdkwork-api-gateway.service sdkwork-knowledgebase-rpc.service \
  /usr/lib/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now sdkwork-knowledgebase-rpc sdkwork-api-gateway

# 5. 验证
curl http://127.0.0.1:3900/healthz
curl http://127.0.0.1:3900/readyz
sudo journalctl -u sdkwork-api-gateway -f
```

启动前必须以 `root:sdkwork`、`0640` 创建
`/etc/sdkwork/database/database.url.secret`，文件内容为经过 URL 编码的 PostgreSQL
连接 URL。Knowledgebase RPC 通过 `SDKWORK_DATABASE_URL_FILE` 读取它，数据库口令
不会进入 systemd unit 或普通 env 文件。

## 端口与环境选择

- 直接部署（本单元，development）：`127.0.0.1:3900`
- 测试环境直接部署：`127.0.0.1:3906`（`etc` / TOML 环境绑定）
- 生产环境直接部署：`0.0.0.0:3900`
- Docker 容器部署占 391x 段（3910/3911/3912/3913 → 容器内 3900）

完整矩阵见 `deployments/README.md`。

## 规范要点

- 单元文件不内嵌环境赋值（`PACKAGING_SPEC.md` §5.5）：安装期生成的值一律放
  `EnvironmentFile=/etc/sdkwork/api-gateway/api-gateway.env`。
- 数据库凭据禁止写入 env 文件、单元文件或配置模板；通过
  `password_file` / `/etc/sdkwork/database/database.secret` / 平台密钥注入。
- **本网关不安装 nginx 站点映射。** 公网反向代理由独立产品
  `sdkwork-webserver` 配置并对上游 `GATEWAY_HOST_PORT` / `127.0.0.1:3900` 转发。

完整的多 OS（Linux/Windows/macOS/容器）安装与配置规范见
`docs/guides/operator/INSTALLATION_STANDARDS.md`。

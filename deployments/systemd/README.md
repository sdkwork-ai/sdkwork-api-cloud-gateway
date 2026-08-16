# systemd 部署（Ubuntu / Debian）

本目录提供 SDKWork API Cloud Gateway 的 systemd 服务单元模板：

- `sdkwork-api-gateway.service` — 服务单元（规范位置
  `/usr/lib/systemd/system/sdkwork-api-gateway.service`，服务名
  `sdkwork-api-gateway`，见 `PACKAGING_SPEC.md` §5.5 安装目标矩阵）
- `sdkwork-knowledgebase-rpc.service` — 网关随包安装的 Knowledgebase 内部 RPC
  sidecar；仅监听 `127.0.0.1:50054`，并通过持久化 mTLS 和签名调用上下文与
  网关中的 IM 模块通信。

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

```bash
# 1. 解压服务包到安装根（归档安装规范路径，RUNTIME_DIRECTORY_SPEC §4.1）
sudo mkdir -p /opt/sdkwork/api-gateway
sudo tar -xzf sdkwork-api-cloud-gateway-<version>-linux-x64-standalone-server.tar.gz \
  -C /opt/sdkwork/api-gateway --strip-components=1

# 2. 创建系统用户与运行时目录（RUNTIME_DIRECTORY_SPEC §4.1）
sudo groupadd --system sdkwork || true
sudo useradd --system --gid sdkwork --home-dir /opt/sdkwork/api-gateway sdkwork || true
sudo install -d -o root -g sdkwork -m 0750 /etc/sdkwork/api-gateway
sudo install -d -o sdkwork -g sdkwork -m 0750 /var/lib/sdkwork/api-gateway \
  /var/log/sdkwork/api-gateway /var/cache/sdkwork/api-gateway /run/sdkwork/api-gateway

# 3. 落位运行时配置（模板 → 规范路径）与环境覆盖文件（0640）
sudo install -o root -g sdkwork -m 0640 \
  config/sdkwork-api-cloud-gateway.production.toml \
  /etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml
sudo touch /etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.env
sudo chmod 0640 /etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.env
sudo editor /etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.env
#   SDKWORK_DATABASE_* 连接注入（密码走 /etc/sdkwork/database/database.secret
#   或 SDKWORK_DATABASE_PASSWORD_FILE，禁止写入本文件，PACKAGING_SPEC §5.5）

# 4. 安装服务单元并启动（PACKAGING_SPEC §5.5 服务名 sdkwork-api-gateway）
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

- 直接部署（本单元，development）：`127.0.0.1:3900`（默认
  `config/sdkwork-api-cloud-gateway.development.toml`）。
- 测试环境直接部署：把 `SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE` 指向
  `config/sdkwork-api-cloud-gateway.test.toml`（`127.0.0.1:3906`），数据库切到
  `sdkwork_ai_test`。
- 生产环境直接部署：使用 `config/sdkwork-api-cloud-gateway.production.toml`
  （`0.0.0.0:3900`），仅在未启用 Docker 容器部署（3910-3913）时使用。
- Docker 容器部署占 391x 段（3910/3911/3912/3913 → 容器内 3900），与本单元共存时
  互不冲突；IM 实时平面 opt-in 监听占 1900x 段（TCP 19000 / UDP 19001 /
  QUIC 19002）。完整矩阵见 `deployments/README.md`。

## 规范要点

- 单元文件不内嵌环境赋值（`PACKAGING_SPEC.md` §5.5）：安装期生成的值一律放
  `EnvironmentFile=/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.env`。
- 数据库凭据禁止写入 env 文件、单元文件或配置模板；通过
  `password_file` / `/etc/sdkwork/database/database.secret` / 平台密钥注入。
- nginx 站点文件放 `/etc/nginx/sites-enabled/sdkwork/<domain>.conf`，上游指向
  `http://127.0.0.1:3900`（`NGINX_SPEC.md`）。

完整的多 OS（Linux/Windows/macOS/容器）安装与配置规范见
`docs/guides/operator/INSTALLATION_STANDARDS.md`。

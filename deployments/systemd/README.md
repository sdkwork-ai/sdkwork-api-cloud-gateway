# systemd 部署（Ubuntu / Debian）

本目录提供 SDKWork API Cloud Gateway 的 systemd 服务单元模板：

- `sdkwork-api-gateway.service` — 服务单元（规范位置
  `/usr/lib/systemd/system/sdkwork-api-gateway.service`，服务名
  `sdkwork-api-gateway`，见 `PACKAGING_SPEC.md` §5.5 安装目标矩阵）

## 安装步骤（Ubuntu）

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
  sdkwork-api-gateway.service /usr/lib/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now sdkwork-api-gateway

# 5. 验证
curl http://127.0.0.1:3900/healthz
curl http://127.0.0.1:3900/readyz
sudo journalctl -u sdkwork-api-gateway -f
```

## 规范要点

- 单元文件不内嵌环境赋值（`PACKAGING_SPEC.md` §5.5）：安装期生成的值一律放
  `EnvironmentFile=/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.env`。
- 数据库凭据禁止写入 env 文件、单元文件或配置模板；通过
  `password_file` / `/etc/sdkwork/database/database.secret` / 平台密钥注入。
- nginx 站点文件放 `/etc/nginx/sites-enabled/sdkwork/<domain>.conf`，上游指向
  `http://127.0.0.1:3900`（`NGINX_SPEC.md`）。

完整的多 OS（Linux/Windows/macOS/容器）安装与配置规范见
`docs/guides/operator/INSTALLATION_STANDARDS.md`。

# 网关 Docker 打包与三环境部署验证报告（2026-09-03）

## 结论

同一镜像 `sdkwork-api-cloud-gateway`（imageId `sha256:ca12d0818638…`，363.7 MB）在
WSL Ubuntu-22.04 内完成构建，并以独立安装 bundle 形式部署 development / test /
production 三个环境，全部 `/healthz` + `/readyz` 200（WSL 内与 Windows 宿主侧均验证通过）。

## 构建链（commit f5795c3）

| 步骤 | 命令 | 结果 |
|---|---|---|
| release 编译+服务包 | `node scripts/package-server.mjs package --deployment-profile standalone` | `dist/server/sdkwork-api-cloud-gateway-0.1.0-linux-x64-standalone-server.tar.gz` |
| OCI 镜像 | `node scripts/build-api-cloud-gateway-container.mjs` | `sdkwork-api-cloud-gateway:local`，manifest digest `sha256:ca12d081863895b5a9fed4f270d58bc09179cebf33e4f86dbf2d212329fec45c` |
| 安装 bundle | `node scripts/docker/package-install-bundle.mjs --skip-image-build` | `dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.1.bundle/`，image.tar.gz sha256 `e25f9f6174c7ad7f883d618687af990389097333067708e51efc6d64362cec70` |

## 部署矩阵（--embedded，单实例）

| 环境 | compose 项目 | 宿主端口 | healthz | readyz | 镜像 digest |
|---|---|---|---|---|---|
| development | `sdkwork-api-cloud-gateway-development-i1` | 3910 | 200 | 200 | ca12d0818638 |
| test | `sdkwork-api-cloud-gateway-test-i1` | 3911 | 200 | 200 | ca12d0818638 |
| production | `sdkwork-api-cloud-gateway-production-i1` | 3913 | 200 | 200 | ca12d0818638 |

访问链接（Windows 宿主浏览器可直接打开）：

- development: http://localhost:3910/healthz （readiness: /readyz）
- test: http://localhost:3911/healthz
- production: http://localhost:3913/healthz

## 旧部署清理

`docker compose -p <project> down -v --remove-orphans` 移除 6 个旧项目
（`sdkwork-gateway-{development,test,staging,production}`、`sdkwork-api-cloud-gateway-demo-i1/-deps`），
并删除 14 个历史孤儿卷；3910–3914 端口已全部释放后复用。

## 本次遇到并解决的问题

1. `target/release/sdkwork-api-cloud-gateway` 被同名空目录占位导致 cargo 链接失败
   （os error 21）→ `rmdir` 后重跑通过。
2. Docker daemon 无代理，直连全部 registry mirror TLS 超时 → 用户态脚本走 shell
   代理拉取 `debian:bookworm-slim`（OCI layout → `docker load`）后构建通过。
3. **production fail-closed（P0-12）**：production 强制数据库连接 `sslmode=require`，
   而 embedded compose 默认连接串硬编码 `sslmode=disable` → 在
   `env/production.env` 注入显式 `SDKWORK_DATABASE_URL`（host=postgres，
   sslmode=require，密码百分号编码）后通过。这是一个值得回修 bundle 模板的缺口
   （embedded 模板应尊重 `GATEWAY_POSTGRES_SSL_MODE`）。
4. production 密钥占位符以 development 示例的 dev-only 默认值填充 + PG 密码由
   deploy.sh 自动生成——**仅适用于本地/冒烟验证，不得用于真实生产**。

## 模板缺口回修与回归（续）

1. **embedded 模板修复**：`docker-compose.bundle.embedded.yml` 两处连接串
   `sslmode=disable` → `sslmode=${GATEWAY_POSTGRES_SSL_MODE:-disable}`（默认值不变，
   dev/test/demo 行为不变；production 置 `GATEWAY_POSTGRES_SSL_MODE=require` 即可
   通过 P0-12，无需再注入显式 `SDKWORK_DATABASE_URL`）。
2. 合约单测补断言锁定该行为：`scripts/deployment-contract.test.mjs` →
   **23 pass / 0 fail**。
3. 部署矩阵回归：`node scripts/validate-docker-deployment.mjs --matrix --compose`
   → **10/10 ok**（development/test/staging/demo/production × embedded/external）。
4. bundle 以修复后模板重打包，production 仅凭 `GATEWAY_POSTGRES_SSL_MODE=require`
   重部署健康（不再依赖 URL 注入 workaround），development/test 同步重新应用，
   三环境 healthz/readyz 复验全 200（WSL 内 + Windows 宿主侧）。
5. bundle 内保留 `prepare-envs.sh`（重建三环境 env：节点 ID 21/22/23、production
   密钥填充与 sslmode 校验）。

## 环境变量差异说明

- `GATEWAY_IM_ID_NODE_ID`：dev=21 / test=22 / production=23（同机多舰队避让）。
- 日常运维：`bash deploy.sh --environment <env> --ps|--logs N|--down --purge`
  （在 bundle 目录内执行）。

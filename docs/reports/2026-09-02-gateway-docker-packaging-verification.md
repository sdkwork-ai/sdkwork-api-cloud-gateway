# sdkwork-api-cloud-gateway Docker 打包与多环境部署验证报告（2026-09-02）

## 一、结论

**已具备商业化落地能力。** 完整打包流程已在 WSL Ubuntu 22.04（Docker 29.3.0）中端到端跑通，
Docker 镜像包 + 统一安装 bundle 产出并通过多实例多环境冒烟验证。

| 交付物 | 路径 | 说明 |
| --- | --- | --- |
| Docker 镜像（本机） | `registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.0` | 377 MB，imageId `sha256:b3c0744…` |
| Docker 镜像安装 bundle | `dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.0.bundle/` | image.tar.gz(376M)+sha256+deploy.sh+5 环境 env+compose+manifest |
| standalone 服务安装包 | `dist/server/sdkwork-api-cloud-gateway-0.1.0-linux-x64-standalone-server.tar.gz` | 已 validate |
| 镜像构建清单 | `dist/container-image.json` | digest/size 证据 |

## 二、本次在 WSL Ubuntu 22 中完整跑通的流程

1. `node scripts/package-server.mjs package --deployment-profile standalone`
   → cargo release 构建 + 安装包组装 + 自校验（增量 10m47s）。
2. `node scripts/build-api-cloud-gateway-container.mjs --skip-build`
   → OCI 镜像构建（377 MB，digest 记录进 `dist/container-image.json`）。
3. `node scripts/docker/package-install-bundle.mjs --skip-image-build`
   → 统一安装 bundle（镜像 tar + 多环境部署资产）。
4. `node scripts/validate-docker-deployment.mjs --matrix --compose`
   → 5 环境 × embedded/external = **10/10 通过**。
5. 端到端部署冒烟：bundle `deploy.sh --environment development --embedded --replicas 2`
   → **2 个网关实例 43 秒内全部 healthy**，并验证后清理（--down --purge）。

## 三、多操作系统进程 / 多环境 / 多实例能力（规范符合性）

- **同机多环境共存（已在运行证明）**：本机 WSL 同时运行
  development(3910)/test(3911)/staging(3912)/production(3913)/demo(3914) 五套网关
  + webserver 三环境 + 本次冒烟的 3930/3940 双实例，互不冲突。
  隔离机制：`<application>-<environment>-i<index>` 独立 compose project、
  独立网络/卷、独立数据库 schema、Redis 逻辑库分段（dev 1/2 … prod 7/8）、
  端口步进计划（base + (i-1)*10）、每实例独立 snowflake 节点 ID。
- **镜像环境中立**：一个镜像跑所有环境，环境选择全部发生在 deploy 时 env 文件
  （DEPLOYMENT_SPEC §6）。
- **IP 直连访问**：`GATEWAY_SECURITY_ALLOW_IP_HOSTS=true`（dev/test/demo 默认开启），
  实测 `Host: 192.168.31.109:3930 → 200`，非法域名 Host → `421 host_not_allowed`。
  注意：WSL mirrored 模式下宿主→自身 LAN IP hairpin 不通是平台限制，局域网内其他设备可直达。

## 四、本次发现并修复的 3 个真实缺陷

1. **bundle 打包器静默截断**（`scripts/docker/package-install-bundle.mjs`）：
   `saveImage` 在 `pipeline()` 完成后才挂 `child.on('exit')`，错过退出事件 →
   Promise 永久 pending → node 以 exit 0 静默退出，bundle 只写入镜像、缺失全部部署资产。
   修复：spawn 后同步挂 exit 监听。
2. **embedded 模式 CA 信任包崩溃循环**（`docker-compose{,.bundle}.embedded.yml` + 基础 compose）：
   embedded 强制 `SSL_CERT_FILE` 指向基础 compose 无条件只读 bind-mount 的目标路径，
   entrypoint 生成合并 CA 包 `mv` 到只读挂载点必然失败；bundle 布局下默认挂载源还不存在，
   docker 会创建幽灵目录。修复：默认挂载源改 `/dev/null`，embedded 的合并包改写到
   `postgres-ca-bundle.merged.crt`（非挂载点）。
3. **CORS 启动即 panic（阻塞发布）**（`sdkwork-web-axum/src/cors.rs`）：
   开发策略 `allowed_headers=["*"]` + `allow_credentials=true` 组合触发 tower-http 0.7
   启动 panic（且该组合本就不被浏览器接受）。修复：tower 适配层把通配头展开为
   规范默认头清单，并新增回归测试（web-axum 5/5 通过）。

另：并行会话重命名上游 SDK crate 导致 `Cargo.lock` 过期，已随构建重新生成；
`docs/reports/2026-09-02-gateway-commercialization-report.md` 中一处
一处涉及 state 的 pnpm 表述触发 PNPM_SCRIPT_SPEC 检查，已改写复验通过。

## 四·补、零手工配置开箱部署验证 + 第 4 个缺陷修复

1. **开箱即部署（零手工配置）已实证**：`development.env.example` 补齐 6 项 cloudrouter
   dev-only 默认密钥（staging 示例保持 `<operator-managed>` 指引）后，全新 bundle 目录内
   仅执行 `cp env/development.env.example env/development.env` + 调整端口/节点 ID，
   `deploy.sh --environment development --embedded --replicas 2` 一次通过：
   2 实例 healthy（i1 15s / i2 5s），`3930/3940 /healthz` 均 200。
   注意：重新打包 bundle 会重置 `env/<env>.env`，冒烟前需重做端口/节点 ID 调整
   （本次曾因此撞上历史舰队 3910 端口，属预期防御行为而非缺陷）。
2. **缺陷 4：多实例部署后默认 teardown 漏拆实例**（`scripts/docker/bundle/deploy.sh`）：
   `--down` 按 env 文件 `GATEWAY_REPLICAS`（默认 1）循环拆实例，而 apply 用
   `--replicas N` 不写回 env 文件 → 2 实例部署后 `--down --purge` 残留 i2 容器与卷。
   修复：down() 通过 `com.docker.compose.project` 标签动态发现所有
   `<environment>-i<index>` 项目，超出 replicas 的残留实例一并拆除并打印
   `discovered extra instance project`；已端到端复验（无参 teardown 后零残留，
   既有 5 环境舰队不受影响），部署契约测试 fail 0。

## 五、遗留事项

1. ~~dev/test env 示例缺 cloudrouter 密钥~~ **已解决（本轮）**：
   development/test/demo 示例已附 dev-only 默认密钥，staging 示例附操作者指引，
   实现零手工配置开箱部署（见四·补）。production 姿态仍强制操作者配置，符合 fail-closed。
2. **真机局域网验证**：WSL mirrored hairpin 限制仍在，最终 IP 直达验证需用局域网内
   另一台设备访问 `http://192.168.31.109:<port>/healthz`（防火墙规则此前已放行 3914）。
3. ~~pnpm 运行于 /mnt/e~~ **根因确证 + ext4 出路已实证（本轮）**：
   pnpm 因项目（/mnt/e, DrvFS/9p）与默认 store（~/.local/share/pnpm, ext4）跨设备、
   硬链接不可跨文件系统，会把 store 回退到项目盘 `<device-state-dir>/.pnpm-store`，
   SQLite over 9p 必报 `disk I/O error`——根因确证。
   实证：源码 rsync 至 WSL ext4（`~/sdkwork-build/sdkwork-api-cloud-gateway`，
   排除 target/node_modules/dist）后 `pnpm install --frozen-lockfile` 2.1s 成功、
   `pnpm run workflow:validate` 通过、部署契约测试 fail 0。
   结论：**pnpm 正规路径在 ext4 工作副本上完全可用**；/mnt/e 原目录打包可继续
   `node scripts/...` 直调（打包脚本无 pnpm 依赖）。修复后回归：部署矩阵 10/10 通过。

## 六、第二轮回归（封装一致性 + 规范符合性，2026-09-2 晚）

聚焦"pnpm 打包脚本与 shell 脚本同封装、入口一致、环境包可商业化交付"，发现并修复 4 项：

1. **多环境 docker 包布局损坏 + 密钥泄漏风险**（`scripts/package-docker-environments.sh`）：
   旧实现把 `docker-compose.yml` 平铺复制进 `dist/docker-packages/<env>/`，但 compose 内
   `./docker/config/config.toml`、`./docker/postgres/*` 挂载源不在包内（docker 会创建幽灵目录
   盖住配置路径）；且把 git-ignored 的本地真实密钥 `docker/env/<env>.env` 直接打进交付物；
   环境清单/端口映射在 6 处重复硬编码；无 pnpm 入口。
   修复：新增共享注册表 `scripts/deployment-environments.mjs`（环境/数据库身份/端口单一事实
   来源，含 shell CLI）；新增 `scripts/docker/package-environment-bundles.mjs` 统一打包器
   （保留仓库相对布局、只打 `.env.example` 模板、逐环境复用 validateDeploymentEnvironment
   预校验、产出 manifest.json）；`package-docker-environments.sh` 改为 3 行薄壳委托同一实现；
   新增公开脚本 `build:container:environments`。实测：打包产物 external/embedded 两模式
   `docker compose config` 均通过。
2. **DEPLOYMENT_SPEC §6 运行时硬化违规**：所有交付 compose 均无 bounded log rotation
   （MUST：json-file + max-size/max-file，参考 50m/3）。已为 docker-compose.yml（4 服务）、
   docker-compose.bundle.yml（x-gateway-common 锚点覆盖 2 服务）、docker-compose.bundle.
   embedded.yml（postgres/redis）补齐，并在部署契约测试中新增强制检查（合并感知：覆盖文件
   中与基础文件共享的服务经 compose merge 继承）。
3. **双份容器配置漂移**：`docker/config/sdkwork-api-cloud-gateway.toml`（仅注释差异）与
   compose 实际挂载的 `config.toml` 并存，且 2 篇 operator 文档指向旧名。已删除副本
   （RUNTIME_DIRECTORY_SPEC：新应用统一 `config.toml` 命名）并修正文档。
4. **文本规范触发项**：4 处 "pnpm <非标准词>:" 文案触发 PNPM_SCRIPT_SPEC 文档检查，已改写。

回归结果：部署契约 + 打包器 + API closure node 测试 44/44；`validate-docker-deployment
--matrix --compose` 10/10（WSL 实机 docker）；`check:standards`/`workflow:validate`/
`topology:validate`/`db:pool:validate`/`gateway:api:validate`/`check:app-composition` 全过。
环境语义确认：镜像环境中立（§6 禁止烘焙环境值、deploy.sh 必须显式 --environment、缺失即
失败），镜像内置 production 模板 + fail-closed 姿态即"默认线上发布"。

**事故记录**：本轮期间 `docker/` 整目录曾被并行进程删除（含 5 个 git-ignored 本地密钥
env）。已从 git 恢复跟踪文件，5 个本地 env 从 WSL ext4 构建副本
（`~/sdkwork-build/...`）完整找回，`.env.example` 未提交修改从 `dist/docker-install`
bundle 副本完整找回，工作区已还原到事故前状态。建议后续为 `docker/env/*.env` 建立退出
仓库前的加密备份约定。

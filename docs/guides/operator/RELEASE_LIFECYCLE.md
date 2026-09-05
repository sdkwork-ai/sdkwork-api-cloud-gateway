# 发布生命周期操作手册（中文）

> 适用对象：需要在 Docker 主机上**发布新版本、升级、回滚、查状态**的运维/开发人员。
> 前置阅读：[Docker 傻瓜式安装指南](docker-install.md)（首次安装用本文档；日常发布用本文档）。

## 一句话总结

日常发布只需要记住一条命令：

```bash
bash release.sh deploy --environment <环境名> --image-tag <版本号>
```

它自带：升级前快照 → 应用新版本 → 健康门禁 → **失败自动回滚** → 版本台账。
出问题时只需要记住另一条：

```bash
bash release.sh rollback --environment <环境名>
```

## 1. release.sh 在哪

| 场景 | 位置 |
| --- | --- |
| 安装包（bundle）部署 | bundle 根目录，`deploy.sh` 旁边：`<bundle>/release.sh` |
| 仓库源码部署 | `scripts/docker/bundle/release.sh`（或 `pnpm release:deploy:standalone:docker -- --environment dev`） |

> 要求：`release.sh` 必须和 `deploy.sh` 在同一目录；bundle 部署时还要求 `compose/`、`env/`、`image.env` 就位（打包脚本自动保证）。

## 2. 六个子命令速查

| 命令 | 作用 | 改动风险 |
| --- | --- | --- |
| `deploy` | 升级/首次发布到指定版本（健康门禁 + 失败自动回滚） | 会变更运行版本 |
| `rollback` | 回滚到上一个成功版本（或 `--to <版本>` 指定） | 会变更运行版本 |
| `status` | 当前版本、容器实际镜像（tag+image ID 双重比对）、每实例健康、漂移检测 | 只读 |
| `history` | 发布台账（谁在什么时候发了什么，结果如何） | 只读 |
| `verify` | 只做健康探测，不做任何变更 | 只读 |
| `versions` | 列出本机可用的镜像版本（deploy/rollback 候选池） | 只读 |

通用参数：`--environment development|test|staging|demo|production`（必填）、`--replicas N`、`--embedded`/`--external`、`--env-file <绝对路径>`、`--dry-run`（只打印计划不执行）。

安全机制（默认开启）：

- **发布互斥锁**：`deploy`/`rollback` 按 environment 加锁（`.lock` 目录，原子创建），并发发布直接报错而非交错执行；等待预算 `--lock-timeout <秒>`（默认 120）。确认没有发布在跑后可删掉 stale 的 `.lock` 目录。
- **bundle 完整性校验**：本机缺镜像、需要从 `image.tar.gz` 加载时，先校验 `image.sha256`（损坏/被篡改的 bundle 会直接拒绝）；`--skip-verify` 可跳过。同时校验目标 tag 与 bundle 携带 tag 一致，避免"加载了 A 却要部署 B"的误导性失败。

## 3. 标准发布流程（升级上线）

以 production 升级到 0.2.0 为例：

```bash
# 0.（可选）预演：只打印将要执行的动作
bash release.sh deploy --environment production --image-tag 0.2.0 --dry-run

# 1. 发布前确认现状
bash release.sh status --environment production

# 2. 发布（目标镜像需已存在于本机，或由 bundle 的 image.tar.gz 提供）
bash release.sh deploy --environment production --image-tag 0.2.0

# 3. 发布后验证
bash release.sh status --environment production
bash release.sh verify --environment production
curl http://127.0.0.1:3913/healthz   # 生产基础端口（每实例 +10）
```

`deploy` 内部按顺序执行：

1. 读取台账中的当前版本（首次发布记为 `-`）；
2. 调用同目录 `deploy.sh --image-tag <版本>` 完成逐实例滚动 apply（实例 1 迁移并健康后，实例 2..N 才启动）；
3. 对每个实例端口 `curl /healthz`（默认预算 180s，可用 `--health-timeout` 调整）；
4. 全部通过 → 写入 `current.json` + 追加台账 `success`；
5. 任一失败 → 追加台账 `failed`，并**自动回滚到升级前版本**（可用 `--no-auto-rollback` 关闭）。

## 4. 回滚

```bash
# 回滚到上一个成功版本（最常用）
bash release.sh rollback --environment production

# 回滚到指定版本（前提：该版本镜像仍在本机或可由 bundle 加载；用 versions 查候选）
bash release.sh rollback --environment production --to 0.1.1
```

- 回滚同样过健康门禁；失败会写台账 `failed` 并立即报错（不会静默）。
- 被回滚掉的版本仍保留在台账里，之后可以再 `--to <版本>` 回去（前向回滚）。
- **⚠️ 数据库迁移不会回滚**：rollback 只替换镜像（代码回退），已执行的数据库迁移保持原样。因此每个版本的迁移必须向后兼容（先扩展后收缩：新版本兼容旧表结构，回滚后旧代码能跑在新表结构上）。这是行业通行约束（Helm/Argo 同理）。
- **跨 bundle 的回滚**：如果新 bundle 的 compose 模板有破坏性变更，直接用旧版本 bundle 目录里的 `release.sh`/`deploy.sh` 执行回滚（镜像 tag 与模板都回到旧版）。因此**建议在主机上保留最近 2~3 个版本的 bundle 目录**。

## 5. 状态与台账

```bash
bash release.sh status --environment production
```

输出示例：

```
environment production
  deployed version : 0.2.0 (at 2026-09-05T02:00:00Z)
  recorded image   : registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.2.0 sha256:abcd...
  topology         : 1 instance(s), host ports 3913 (+10 per instance) -> container 8080, deps=external
  instance 1 : port 3913 HEALTHY
  drift            : none (containers match the recorded release)
```

`drift` 非空说明有人绕过 release.sh 直接改了容器（例如手动 `docker compose up`），需排查。

台账文件（append-only，可直接归档）：

```
<bundle>/release-state/<environment>/ledger.jsonl   # 每行一条 JSON 记录
<bundle>/release-state/<environment>/current.json   # 当前版本快照
```

自定义台账位置：`export SDKWORK_RELEASE_STATE_DIR=/var/lib/sdkwork-gateway-release`（外部 `--env-file` 模式默认放在 env 文件旁的 `release-state/`，例如 `/etc/sdkwork/api-gateway/docker/release-state/`）。

## 6. 日常开发流程怎么接

| 环节 | 做法 |
| --- | --- |
| 开发自测 | `pnpm dev`（本地进程，不走发布体系） |
| 构建新版本 | 构建机上 `bash scripts/build-install-bundle.sh --version 0.2.0`（产物：`dist/docker-install/...-0.2.0.bundle`） |
| 传到目标主机 | `scp -r` 或复用 `scripts/docker/remote-deploy.sh`；旧版本 bundle 目录保留不删 |
| test 环境先发 | `bash release.sh deploy --environment test --image-tag 0.2.0` + `verify` |
| staging / production | 同上，逐环境推进；每一步 `status` 留痕 |
| 出问题 | `bash release.sh rollback --environment <env>`（秒级回到上一版本） |
| 停机/启动/重启 | 仍用 `deploy.sh --stop / --start / --restart`（这些是运维动作，不是发布动作） |

## 7. 常见问题

- **`deploy` 报 "image ... not found locally"**：目标版本镜像不在本机。用对应版本 bundle 里的 `image.tar.gz`（`docker load -i image.tar.gz`），或先在构建机产出并搬运；`release.sh versions --environment <env>` 可查本机候选。
- **`deploy` 报 "another release operation holds ... .lock"**：同一环境有发布正在进行。等它结束；确认没有发布在跑后可删除 stale 的 `.lock` 目录重试。
- **`deploy` 报 "this bundle carries ... not ..."**：`--image-tag` 指定的版本与本 bundle 携带的镜像不一致。到目标版本的 bundle 目录里执行，或先构建/搬运目标镜像。
- **`rollback` 报 "no previous successful version"**：该环境从未记录过发布（台账为空）。第一次请用 `deploy` 建立基线。
- **升级失败已自动回滚，但我想看失败原因**：`bash deploy.sh --environment <env> --logs 1` 查看实例日志，`bash release.sh history --environment <env>` 看失败记录。若失败版本就是当前版本（幂等重发布失败），自动回滚会明确提示跳过（回滚到自己没有意义），修完根因重新 deploy 即可。
- **`--replicas` 扩缩容**：直接 `deploy.sh --environment <env> --replicas 3`；release.sh 的 `deploy` 也会透传 `--replicas`。
- **健康门禁超时太短/太长**：`--health-timeout <秒>`（默认 180）。
- **灰度/金丝雀发布的局限**：当前为单机 docker compose 拓扑，`deploy` 按实例顺序滚动（实例 1 健康后才启动 2..N），已提供基本的滚动保护；但与 Argo Rollouts 类的金丝雀按比例引流需要网关层支持，本体系未覆盖。多实例生产环境建议先在 test/staging 全量验证。

## 8. 与规范的关系

- 打包链路：`DEPLOYMENT_SPEC.md` §6 / `PNPM_SCRIPT_SPEC.md` §4.4（不变）。
- release.sh 是 `deploy.sh` 的生命周期封装，不改变 compose 拓扑、端口计划与健康检查契约（`GATEWAY_CONTAINER_PORT` attach 契约不变）。
- 台账属运行时状态，不入库不入 git（`.gitignore` 已排除 `release-state/`）。

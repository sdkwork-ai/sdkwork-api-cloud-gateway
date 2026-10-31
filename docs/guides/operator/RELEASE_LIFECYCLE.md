# 发布生命周期操作手册（中文）

> 适用对象：需要在 Docker 主机上**发布新版本、升级、回滚、查状态**的运维/开发人员。
> 前置阅读：[Docker 傻瓜式安装指南](docker-install.md)（首次安装用那份；日常发布用本文档）。
> 唯一操作通道：`bin/` 九入口（`MODULE_BIN_SPEC.md`）。所有发布动作（含目标机上的滚动 apply、健康门禁、台账记录）都由 `bin/docker-deploy.sh` 驱动完成；bundle 内的执行器是 `bin/` 的内部实现，操作员**不需要也不应该**手动调用任何脚本。

## 一句话总结

日常发布只需要记住两条命令：

```bash
bin/docker-image.sh build                                          # 构建规范镜像（tag 取自 agent.app.config.json）
bin/docker-deploy.sh upgrade --environment production --image-tag <版本号>   # 升级（含健康门禁 + 失败自动回滚 + 台账）
```

出问题时只需要记住另一条：

```bash
bin/docker-deploy.sh rollback --environment production             # 秒级回到上一个成功版本
```

远程主机加 `--host ssh://[user@]host[:port]`；全部变更类命令可先加 `--dry-run` 打印计划；production 必须显式 `--yes`。

## 1. 命令速查（全部经 bin/）

| 动作 | 命令 | 改动风险 |
| --- | --- | --- |
| 升级/首次发布 | `bin/docker-deploy.sh install --environment <env>`（生产加 `--yes`） | 会变更运行版本 |
| 升级到指定版本 | `bin/docker-deploy.sh upgrade --environment <env> --image-tag <版本>` | 会变更运行版本 |
| 回滚 | `bin/docker-deploy.sh rollback --environment <env> [--to <版本>]` | 会变更运行版本 |
| 状态/漂移检测 | `bin/docker-deploy.sh status --environment <env>` | 只读 |
| 健康探测 | `bin/doctor.sh --environment <env>` | 只读 |
| 查看日志 | `bin/docker-deploy.sh logs --environment <env> [--tail N] [--follow]` | 只读 |
| 停止/启动/重启 | `bin/docker-deploy.sh stop/start/restart --environment <env>` | 运维动作 |
| 下线 | `bin/docker-deploy.sh down --environment <env> [--purge --yes]` | 会删除栈 |

## 2. 内置安全机制（默认开启，无需配置）

- **发布互斥锁**：同一环境的发布/回滚按环境加锁，并发发布直接报错而非交错执行；确认没有发布在跑后可清理 stale 锁目录重试。
- **bundle 完整性校验**：本机缺镜像、需要从 bundle 的 `image.tar.gz` 加载时，先做 sha256 校验（损坏/被篡改的 bundle 直接拒绝）；同时校验目标 tag 与 bundle 携带 tag 一致，避免"加载了 A 却要部署 B"。
- **健康门禁**：逐实例滚动 apply（实例 1 完成数据库供给与迁移并健康后，实例 2..N 才启动），每实例探测 `/healthz`（预算 180s）。
- **发布台账**：append-only 记录每次发布/回滚（谁在什么时候发了什么、结果如何）；升级失败自动回滚到升级前版本并记录 `failed`。

## 3. 标准发布流程（以 production 升级到 0.2.0 为例）

```bash
# 0.（可选）预演：只打印将要执行的动作
bin/docker-deploy.sh upgrade --environment production --image-tag 0.2.0 --dry-run

# 1. 发布前确认现状
bin/docker-deploy.sh status --environment production

# 2. 构建并发布（镜像由 bin 构建并同步；目标机通过 --host 指定）
bin/docker-image.sh build
bin/docker-deploy.sh upgrade --environment production --image-tag 0.2.0 --yes

# 3. 发布后验证
bin/docker-deploy.sh status --environment production
bin/doctor.sh --environment production
curl --noproxy '*' http://127.0.0.1:3913/healthz   # 生产基础端口（每实例 +10）
```

## 4. 回滚

```bash
bin/docker-deploy.sh rollback --environment production             # 回到上一个成功版本（最常用）
bin/docker-deploy.sh rollback --environment production --to 0.1.1  # 回到指定版本（镜像需在本机或可由 bundle 提供）
bin/docker-deploy.sh status     --environment production
```

- 回滚同样过健康门禁；失败会记入台账并立即报错（不会静默）。
- 被回滚掉的版本仍保留在台账里，之后可以再 `--to <版本>` 回去（前向回滚）。
- **⚠️ 数据库迁移不会回滚**：rollback 只替换镜像（代码回退），已执行的数据库迁移保持原样。因此每个版本的迁移必须向后兼容（先扩展后收缩：新版本兼容旧表结构，回滚后旧代码能跑在新表结构上）。这是行业通行约束（Helm/Argo 同理）。
- **跨 bundle 的回滚**：如果新 bundle 的 compose 模板有破坏性变更，`bin/docker-deploy.sh rollback` 会拒绝；此时用旧版本的镜像 tag 通过 `bin/docker-deploy.sh upgrade --image-tag <旧版本>` 回退（镜像与运行配置都回到旧版）。**建议在主机上保留最近 2~3 个版本的镜像**。

## 5. 状态与台账

```bash
bin/docker-deploy.sh status --environment production
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

`drift` 非空说明有人绕过 `bin/` 直接改了容器（例如手动 `docker compose up`），需排查。

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
| 构建新版本 | 构建机上 `bin/docker-image.sh build`（镜像 + 构建证据） |
| 发到目标主机 | `bin/docker-deploy.sh install/upgrade --environment <env> --host ssh://ops@<主机>`（bundle 同步、镜像加载、健康门禁一条命令完成） |
| test 环境先发 | `bin/docker-deploy.sh upgrade --environment test --image-tag <版本>` + `status` |
| staging / production | 同上，逐环境推进；每一步 `status` 留痕；生产加 `--yes` |
| 出问题 | `bin/docker-deploy.sh rollback --environment <env>`（秒级回到上一版本） |
| 停机/启动/重启 | `bin/docker-deploy.sh stop/start/restart --environment <env>` |

## 7. 常见问题

- **升级报 "image ... not found locally"**：目标版本镜像不在目标机。`bin/docker-image.sh save -o dist/image.tar.gz` 后在目标机 `bin/docker-image.sh load -i image.tar.gz`，或直接让 `bin/docker-deploy.sh install --host ...` 完成 bundle 同步与镜像加载。
- **升级报 "another release operation holds ... .lock"**：同一环境有发布正在进行。等它结束；确认没有发布在跑后可删除 stale 的锁目录重试。
- **升级报 "this bundle carries ... not ..."**：`--image-tag` 指定的版本与本 bundle 携带的镜像不一致。先 `bin/docker-image.sh build` 产出目标版本，再重试。
- **回滚报 "no previous successful version"**：该环境从未记录过发布（台账为空）。第一次请用 `install/upgrade` 建立基线。
- **升级失败已自动回滚，但我想看失败原因**：`bin/docker-deploy.sh logs --environment <env> --tail 200` 查看实例日志，`bin/docker-deploy.sh status` 看台账结果。若失败版本就是当前版本（幂等重发布失败），自动回滚会明确提示跳过，修完根因重新升级即可。
- **扩缩容**：`bin/docker-deploy.sh install --environment <env> --replicas 3`。
- **灰度/金丝雀发布的局限**：当前为单机 docker compose 拓扑，升级按实例顺序滚动（实例 1 健康后才启动 2..N），已提供基本的滚动保护；但与 Argo Rollouts 类的金丝雀按比例引流需要网关层支持，本体系未覆盖。多实例生产环境建议先在 test/staging 全量验证。

## 8. 与规范的关系

- 操作通道契约：`MODULE_BIN_SPEC.md` §4.1/§4.2；bundle 契约：`DOCKER_SPEC.md` §4、`OPERATIONS_SPEC.md` §1.2。
- 发布台账机制不改变 compose 拓扑、端口计划与健康检查契约（`GATEWAY_CONTAINER_PORT` attach 契约不变）。
- 台账属运行时状态，不入库不入 git（`.gitignore` 已排除 `release-state/`）。

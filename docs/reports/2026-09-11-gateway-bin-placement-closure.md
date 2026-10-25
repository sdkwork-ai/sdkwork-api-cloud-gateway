# gateway bin/ 单入口面收口与门禁盲区修补验证报告

- 日期：2026-09-11
- 范围：`sdkwork-api-cloud-gateway`（`bin/` 归置 + 死代码清除）+ `sdkwork-specs`（`tools/check-script-placement.mjs` 盲区修补）
- 结论：**通过**。gateway 放置违规 **11 → 0**；死代码净删 ~380 行 + 9 个文件；gateway 门禁 4/4 全绿、测试 **77/77**；舰队放置违规 **137 → 132**，同时新暴露 **6 条真实无扩展名脚本违规**（另有 2 条 Gradle wrapper 误报已豁免）。

## 1. 目标

`MODULE_BIN_SPEC.md` §2.1 要求「每个手写脚本都住在 `bin/` 下，产物里的副本由构建拷出」。上一轮已完成 4 个模块的 bundle executor 平铺化，但 gateway 仍有 11 条 placement 违规、一处 `bin/bundle` 式旧布局残留，以及一条 `bin/docker-bundle-deploy.sh --prepare-envs` 缺失的执行器。本轮收口到 **0 违规**，并清除归置过程中暴露的冗余与死代码。

## 2. 最终 `bin/` 布局

```text
bin/                                 九入口（唯一 operator surface）
  docker-image.sh  docker-deploy.sh  config.sh  doctor.sh  backup.sh
  apps-build.sh  apps-package.sh  apps-deploy.sh  apps-pkg-installer.sh
  README.md
  lib/            bootstrap.sh  module.sh
  container/      entrypoint  knowledgebase-rpc-entrypoint
                  postgres/entrypoint.sh  postgres/init/001-create-schema.sh
  host/           README.md  prepare-host.sh  provision-external-databases.sh
                  external-schema.sql  wsl-reset-db-passwords.sh  bind-windows-hosts.ps1
  packaging/      install-ubuntu.sh  build-install-bundle.sh  validate-ubuntu-package.sh
  docker-bundle-deploy.sh  docker-bundle-release.sh  docker-bundle-prepare-envs.sh
```

构建产物路径**刻意保持不变**：`Dockerfile` 仍把 `bin/container/*` 拷进镜像的 `container/`；运行时仍是
`/opt/sdkwork/api-gateway/container/entrypoint`；bundle 里仍是 `deploy.sh` / `release.sh` / `prepare-envs.sh` 与 `postgres/`。

## 3. 删除清单（每条都有消费者证据）

| 路径 | 判定依据 |
| --- | --- |
| `bin/lib/deploy-environments.sh` | 与 `scripts/deployment-environments.mjs`（自declare 为 single source of truth，且自带 shell CLI）完全重复的 shell 注册表 |
| `bin/lib/deploy-common.sh` | 唯一消费者是上面那个文件 |
| `bin/host/generate-certs.sh` | 唯一消费者 `install-certs.sh` 已在前一轮删除；输出 `docker/certs/<env>/*.pem` 全仓无任何 mount/引用；CA 签发职责已由 `prepare-host.sh` 第 4 步自带完成 |
| `docker/nginx/api{,-dev,-test,-staging}.sdkwork.com.conf` | gateway 不拥有 nginx（契约测试已断言）；引用的证书路径无产者 |
| `.gitignore` 的 `docker/certs/` 条目 | 对应目录已无产者 |
| 空目录 `docker/postgres/`、`docker/scripts/`、`deployments/docker/` | 迁移后的目录残留 |

迁移（非删除）：`docker/postgres/external-schema.sql` → `bin/host/external-schema.sql`；`container/{entrypoint,knowledgebase-rpc-entrypoint}` → `bin/container/`。

## 4. 归置暴露的真 bug（均已修）

| 文件 | 问题 | 修法 |
| --- | --- | --- |
| `scripts/package-server.mjs` | 服务器包仍从 `scripts/install-ubuntu.sh` 拷贝，该文件已迁 `bin/packaging/` ⇒ 打包必 ENOENT | 改指 `bin/packaging/install-ubuntu.sh` |
| `scripts/build-api-cloud-gateway-container.mjs` | `--check` 在 plan **无** issue 时会穿透到真正的 cargo+docker 构建 | `if (settings.check) return issues>0 ? 1 : 0`，对齐 usage「Validate the build plan without building」 |
| `scripts/dev/...-source-config.mjs` | `lifecycleEnvironments` 硬编码 4 环境，缺 `demo`，与五环境 canon 漂移 | 改为 `new Set(ENVIRONMENTS)`（引 `../deployment-environments.mjs`） |
| `scripts/dev/...-source-config.test.mjs` | 断言 `publicHosts.length === 14`，实际 16（域名族早已扩到 8 品牌 ×2 TLD） | 改 16，并把 `demo` 补进环境循环 |
| `scripts/release-evidence.mjs` | `gitResolvedDependencies` 对任一依赖仓 `git rev-parse HEAD` 失败即整体抛错（本机 `sdkwork-im` 仓 HEAD unborn 即踩） | 改为 skip 该仓并注释；SBOM 仍含全部 registry 锁包 |
| `scripts/deployment-contract.test.mjs` | 6 处断言仍指 `container/*`、`docker/postgres/*`、`deployments/docker/bundle/*` | 全部改指 bin/ 源路径 |

另有 rename 后必错的一类路径深度修复：`bin/lib/*`（`../../`）、`bin/host/{prepare-host,provision-external-databases}.sh`（`../..`）、`bin/packaging/build-install-bundle.sh`（`SCRIPT_DIR/../..`）。`bin/host/wsl-reset-db-passwords.sh` 原本已写对。

## 5. 门禁盲区修补（`sdkwork-specs`）

`tools/check-script-placement.mjs` 的 `isScript()` 只看后缀（`.sh/.bash/.zsh/.ksh/.ps1`），**看不到无扩展名的 image `ENTRYPOINT`** —— 这正是 `container/entrypoint` 能长期躺在仓根而不被门禁发现的原因，而 §2.1 的表里明确把 image ENTRYPOINT 归给 `bin/container/`。

新增：**无点号文件名 + 头两字节为 `#!` ⇒ 视为脚本**；并豁免 `gradlew` / `mvnw`（构建工具自动生成的 wrapper，属「生成物不是源」这一既有类别，命名豁免以免制造误报）。

## 6. 验证结果

### gateway（`--root .`）

| 门禁 / 测试 | 结果 |
| --- | --- |
| `check-script-placement` | **0 违规**（原 11）；审计脚本 139 → **141**（新识别 2 个 entrypoint） |
| `check-module-bin` | passed |
| `check-operations-conformance` | **12 PASS / 0 FAIL** |
| `check-shell-portability` | 22 files / 0 findings（`bash -n` 全开） |
| `bash -n` 全部 `bin/**/*.sh` + `sh -n` 两个 container entrypoint | 通过 |
| 6 个测试套件 | **77/77**（`deployment-contract` 23/23） |
| WSL 下 `build-api-cloud-gateway-container.mjs --check` | exit 0，无 validation issue（证明 `bin/container/entrypoint` 双存在性检查在 Linux 侧成立） |

### 舰队（`--workspace`，100 roots）

| 指标 | 基线 | 现在 |
| --- | --- | --- |
| 审计脚本 | 1610 | **1615** |
| 放置违规 | 137 | **132** |
| 模块对齐 | 75/100 | **76/100** |

违规净减 5 = gateway 修掉 11，加上盲区修补新暴露 6 条真实违规。新暴露项（**均为 bin/ 之外的无扩展名脚本**）：

- `sdkwork-birdcoder`：`sdkwork-run-node`、`sdkwork-run-pnpm`
- `sdkwork-agentstudio`：`sdkwork-run-node`、`sdkwork-run-pnpm`
- `sdkwork-superpowers`：`hooks/session-start`
- `sdkwork-terminal`：`apps/sdkwork-terminal-pc/packages/sdkwork-terminal-pc-shell/prepack`

误报校验：修补后首跑出现 `sdkwork-birdcoder` / `sdkwork-utils` 各 1 条 `gradlew`，加豁免后消失，模块对齐数由 75 → 76，说明豁免精确、无副作用。

## 7. 遗留

1. 上述 6 条新暴露违规待逐条判定「迁 `bin/`」还是「豁免」，属舰队范围（非 gateway）。
2. `MODULE_BIN_SPEC.md` §2 的「薄包装 ≤8 非注释行」当前只作用于九个必需入口；把该规则扩展到全部 `bin/*.{sh,ps1,cmd}` 仍是待办（须在各模块 `bin/` 全部落到合规族之后）。
3. `agent.app.config.json` 历史 release 说明仍写「bundle source moved … to deployments/docker/bundle」，现已再进一步到 `bin/` 平铺；历史条目不改写，随下次发布更新描述。

## 8. 事故复盘（程序性教训，非代码问题）

本轮执行中出现一次**目录级误删**：`git rm docker/nginx/*.conf && rmdir docker/nginx` 一条命令把整个 `docker/` 删掉（与上一轮 `scripts/` 被摧毁同源的 MSYS/文件系统 shim 路径事故）。

- 恢复：`git checkout -- docker` 找回全部已跟踪文件；但该命令**同时回滚了 `docker/README.md` 上一轮尚未提交的改动**，已重新应用。
- 未被跟踪/被 gitignore 的 `docker/env/{development,test,staging,demo,production}.env`（含真实密钥）与 `docker/ca/postgres-ca-bundle.crt` 一度丢失。恢复来源：`.workbuddy/backup/cors-20260911/gateway-env/*.env`（按已知行号逐行校验一致：staging:49 / production:55 / demo:45）；CA bundle 按 `bin/docker-bundle-deploy.sh` 自带口径 `cat /etc/sdkwork/postgres-tls/{ca.crt,server.crt}` 重建（2453 B = 1164 + 1289，完整）。
- 三条铁律：① 删除/门禁类命令不要用绝对路径参数，删后立刻 `find <dir>` 复核；② `git checkout -- <dir>` 会丢弃该目录下全部未暂存修改；③ operator 密钥目录必须有仓内快照。

为降低「误 checkout 回滚未提交工作」的风险，本轮结束时已把两个仓库的**索引与工作区对齐**（gateway `git add -A`；specs 仅暂存 `tools/check-script-placement.mjs`），未做提交。

# CORS 标准全舰队对齐与门禁接线验证报告

- 日期：2026-09-11
- 范围：`sdkwork-specs`（权威规范与工具）+ 78 个模块仓（`etc/topology` 载体）+ 工作区根聚合
- 结论：**通过**。78 模块 / 777 载体 / 0 问题；68 个模块已接入仓级门禁；工作区根 `pnpm check:all` 含 CORS 门禁；新增域名全链路演练 PASS。

## 1. 背景与目标

各模块 `etc/topology/*.env` 各自配置大量域名，CORS 白名单此前没有权威派生口径，导致：

- 载体缺失、域名漂移、生产混入明文 `http` 与 loopback、客户端源站缺失、顺序非规范、弃用 app 级键残留；
- 不同模块写法不一致，新增域名靠"各处手改"，必然遗漏。

本轮目标：把 CORS 配置收敛到 `sdkwork-specs` 单一权威，并使"对齐 specs 配置"成为处理新增域名的唯一动作。

## 2. 权威分层（现行）

| 层 | 文件 | 职责 |
| --- | --- | --- |
| 注册表 | `tools/webserver/host-registry.mjs` | 16 个基础域（8 品牌 × `.com`/`.cn`）、appId→角色标签、环境后缀 |
| 派生算法 | `tools/cors/registry.mjs` | 唯一源站派生：host 块 / loopback 种子 / 边缘种子 / 客户端源站 / 保留额外源站 / 顺序 |
| 载体发现与审计 | `tools/cors/scan.mjs` | 载体目录发现、逐载体结构审计、**门禁接线审计** |
| 外科式 .env 读写 | `tools/cors/env-file.mjs` | CRLF 感知、锚点插入、顺序保持的键改写 |
| 门禁 | `tools/check-cors-standard.mjs` | `--workspace <root>`（工作区）/ `--root .`（单仓） |
| 对齐器 | `tools/align-cors-standard.mjs` | 物化载体；`--check` 干跑 / `--fix` 写入 |
| 规范 | `CORS_SPEC.md` §3–§10 | 公式、契约、流程、门禁、验收清单 |

派生规则要点：

- 生产 = 每个 browser-facing surface × 已发布基础域 → `https://<host>`（无 http）；
- 非生产 = loopback 开发种子 + 每 host 的 `https`/`http` **成对**；
- 客户端源站固定 6 个（`app://dsh|birdcoder|sdkwork|dtupay`、`tauri://localhost`、`https://servicewechat.com`）每环境必有；
- **边缘 loopback 种子**（`http://localhost:<边缘端口>` / `http://127.0.0.1:<边缘端口>`，dev 3910 / test 3911 / staging 3912 / demo 3914 / prod 3913）**每个环境都保留**，属边缘清单键，不进模块自有清单；
- surface 选择器用 `cloudPublicHosts`，**不是** profile 的 `healthSurfaces`（否则 webserver 丢 `server-app` / `server-admin`）。

## 3. 本轮改动

### 3.1 规范与工具（`sdkwork-specs`）

- `tools/cors/scan.mjs`：新增 `collectCarriers` / `readModule` / `readScripts` / `corsGateState` / `auditGateWiring` / `auditModule`；`auditWorkspace` 返回值新增 `wiring`。
- `tools/check-cors-standard.mjs`：`--root <dir>` 改为**单仓模式**（不扫父工作区，找不到载体则 fail-closed）；输出新增接线审计；摘要新增 `gate wiring errors / warnings`。
- `tools/align-cors-standard.mjs`：`--root <dir>` 单仓模式对齐（`--workspace` 行为不变）。
- `tools/cors/registry.test.mjs`：新增边缘种子用例，**22 个用例全绿**。
- `CORS_SPEC.md`：§6 增补"无门禁的载体是隐性故障"；§7.1 增补单仓 `--root .` 与单测命令；§8 重写门禁表、新增 **§8.1 Gate Wiring（规范性）**；§10 增补 3 条验收项。
- `README.md`：规范行与任务路由行补上门禁接线契约。

### 3.2 仓级门禁接线（68 个模块）

每仓新增：

```json
"check:cors-standard": "node ../sdkwork-specs/tools/check-cors-standard.mjs --root ."
```

并按 `_sdkwork:verify` → `verify` → `check` 的优先级，把该步骤接到 `pnpm verify` 实际运行的聚合上（跟随该仓既有 runner 风格：`pnpm` / `pnpm.cmd` / `sdkwork-run-pnpm`）。

- 接入：**68** 个模块（66 个批量 + notary / im 试点）。
- 未接入：**9** 个模块没有任何 `_sdkwork:verify`/`verify`/`check` 聚合（`birdcoder2`、`drama`、`generations`、`image`、`messaging`、`sandbox`、`search`、`video`、`web-framework`），属 `PNPM_SCRIPT_SPEC.md` 既有缺口，门禁以 **warning** 报出；其载体仍由工作区门禁覆盖。
- `sdkwork-tts` 无 `package.json`，同样为 warning。

### 3.3 工作区根聚合（`@sdkwork/workspace-root`）

```json
"check:cors-standard": "node sdkwork-specs/tools/check-cors-standard.mjs --workspace .",
"align:cors-standard": "node sdkwork-specs/tools/align-cors-standard.mjs --workspace . --fix",
"test:cors": "node --test sdkwork-specs/tools/cors/registry.test.mjs",
```

`check:all` 末尾追加 `&& pnpm run check:cors-standard && pnpm run test:cors`。

## 4. 验证证据

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| 派生算法单测 | `node --test tools/cors/registry.test.mjs` | 22 / 22 通过 |
| 工作区门禁 | `check-cors-standard --workspace ..` | 78 模块、777 载体、0 问题、0 接线错误、10 警告 |
| 边缘本地权威 | `derive-cors-allowed-origins.mjs --check`（gateway） | 29 载体全部 canonical |
| 仓级门禁（抽样） | `check-cors-standard --root .` | notary 10 / im 15 / webserver 19 / mall 4 载体，各 0 问题 |
| 脚本命名合规 | `check-pnpm-script-standard.mjs --root .` | notary / im / webserver 均 ok |
| 工作区根 | `pnpm check:cors-standard` / `pnpm test:cors` | exit 0 / 22 通过 |
| 门禁负向验证 | 向 `im` 的 standalone.development 注入 `https://evil.example.com` | exit 1，报 `outside the registered product domain family` + 顺序错误；还原后 0 问题 |
| 接线负向验证 | 摘掉 `forum` 的 `check:cors-standard` | 报 `check:cors-standard is missing`，1 个接线错误 |
| **新增域名演练** | `cors-newdomain-drill.mjs`（沙盒） | **PASS**，见下 |

### 4.1 新增域名全链路演练（沙盒，零风险）

在一次性沙盒中复现 `CORS_SPEC.md` §7.1：

1. 基线（16 个注册域）：门禁 exit 0；
2. 注册第 17 个域 `drill-cors.com`，按 webserver 对齐器的产物物化 240 条 host 条目（10 个 profile）；
3. **未对齐时门禁 exit 1**：10/10 载体报 `missing derived origin https://notary-demo.drill-cors.com` 等；
4. `align-cors-standard --root . --fix` → 门禁 **exit 0**，0 问题；
5. 二次对齐 `--check` → `0 carrier(s) not canonical`（幂等）；
6. `standalone.development` 实际带上 4 条新域源站（2 surface × `https`/`http` 成对）。

## 5. 遗留事项

1. 9 个无聚合模块需按 `PNPM_SCRIPT_SPEC.md` 补齐 `_sdkwork:verify`/`verify` 标准面；补齐后门禁自动要求接线（当前为 warning，非阻塞）。
2. 跨 73 个仓的 CORS 载体对齐与 68 个仓的门禁接线**尚未提交 git**；上一阶段 4 个未推送提交仍待决定。
3. `production` 环境 cloudrouter 六件套仍为 development 占位密钥（既有项）。
4. `staging`/`production` 缺 `SDKWORK_KNOWLEDGEBASE_OUTBOX_WEBHOOK_URL`，每 10s 刷 ERROR（既有项，非本轮回归）。

## 6. 复现命令

```sh
# 全量对齐 + 门禁
cd sdkwork-specs
node tools/align-cors-standard.mjs --workspace .. --check     # 干跑
node tools/align-cors-standard.mjs --workspace .. --fix       # 写入
node tools/check-cors-standard.mjs --workspace ..
node --test tools/cors/registry.test.mjs

# 单仓（各模块 pnpm verify 内已接线）
node ../sdkwork-specs/tools/check-cors-standard.mjs --root .
node ../sdkwork-specs/tools/align-cors-standard.mjs --root . --fix

# 工作区根
pnpm check:cors-standard && pnpm test:cors
```

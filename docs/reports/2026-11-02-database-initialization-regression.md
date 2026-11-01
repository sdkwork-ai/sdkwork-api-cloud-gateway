# 数据库初始化全量回归（2026-11-02）

范围：`D:\sdkwork-space` 下全部 `sdkwork-*` 数据库模块。
目标：验证每个模块在**全新 canonical PostgreSQL 库**上完成 init → migrate → seed，
二次执行幂等；核对 dev / test / staging / demo / production 五套环境身份与发现输入；
修复缺失或错误的初始化数据。

## 1. 方法与矩阵

| 维度 | 执行内容 |
| --- | --- |
| 模块清单 | 71 个声明 `database/database.manifest.json` 的模块：69 个 PostgreSQL，2 个 sqlite-only（agentstudio、video-cut，不参与 PG 回归） |
| 回归条目 | 78 条 = 69 个 root 模块 + 9 个声明子模块（appbase `edu-data`/`med-data`，cloudrouter 7 个子模块） |
| 首次初始化 | 在全新 `sdkwork_ai_prod` 上按 gateway `databaseBootstrapOrder` + 字母序逐条 `sdkwork-db bootstrap` |
| 幂等 | 全量重跑第二遍，要求 `0 migration(s), 0 seed script(s)` |
| 行数校验 | 188 张 seed 目标表逐一 `count(*)`（缺失表记 -1） |
| development | 对 20 个 `seedOnBoot=true` 的 PG 模块在既有 `sdkwork_ai_dev` 上收敛执行 |
| staging / demo | 新建 canonical `sdkwork_ai_staging` / `sdkwork_ai_demo`，跑 5 个代表模块（payment/appbase/iam/appstore/membership），校验后删除 |
| test | 配置层校验：`canonical_database_profile` 将 `sdkwork_ai_test`、`sdkwork_ai_test_*` 映射到 test profile；共享测试角色凭据归测试工具链所有，本次不重置 |

工具：WSL Rust 构建的 `sdkwork-db`（`sdkwork-database-cli`），与容器/systemd 使用的是同一
`LifecycleOrchestrator`。宿主 Windows Rust 工具链缺少 `rustc`，无法在 Windows 侧重编 gateway，
因此 gateway 的差异以代码走查 + 单元测试 + 数据库生命周期实测覆盖。

## 2. 结果总览

| 项目 | 结果 |
| --- | --- |
| 生产型全新库首次初始化 | 78 条中 76 条通过；2 个模块为已知阻塞（见 §4） |
| 二次幂等 | 76 条通过条目全部 `0 migration(s), 0 seed script(s)` |
| dev 收敛 | 20 个 `seedOnBoot=true` 模块：18 通过，2 个为环境历史问题（见 §4） |
| staging / demo | 5/5 模块全部通过，数据行数与 prod 一致 |
| 发现的缺陷 | 12 类，其中 10 类已修复并提交（24 个仓库） |

## 3. 已修复

| 缺陷 | 影响 | 修复 |
| --- | --- | --- |
| `seedOnBoot=false` 覆盖 14 个承载权威初始化数据的模块 | 全新环境 base 字典、支付目录、系统默认值、技能、企业/需求、社区圈子、商品目录、论坛、游戏目录、邮件模板、MCP 分类、伙伴等级、驱动翻译、默认网关拓扑全部为空 | 14 个模块 manifest 置 `seedOnBoot=true`（commit 见 §6） |
| 发现输入键名不匹配 | 容器镜像 8 个 surface 的 `SDKWORK_<MODULE>_APP_ROOT` 与 Rust 派生规则不一致（appbase/deployments/customerservice/gameengine 等），这些 surface 在容器中从不被发现；`pnpm dev` 完全未声明任何 surface，只初始化 documents/models 回退目录 | Dockerfile 补齐/修正 canonical 键；dev runner 扫描同级模块自动导出（含单测） |
| host-native 安装器不写 surface 根 | 直装生产方式 discovery 选不中任何 surface，`seedOnBoot` 模块不灌数据 | 安装器按 `database-modules/*` 生成 `SDKWORK_<MODULE>_APP_ROOT` |
| appstore `010_download_capabilities` 用 `listing_status='published'` | 005/017 归一化为 `active` 后，全新库 download grant / install event 为 0 | 接受 `active` 与 legacy `published` |
| appstore 005 写入 `published` | 全新库 78/235 个上架在目录查询中不可见 | 写入 `active` 并追加 legacy 归一化 UPDATE |
| company demand hall seed 双重编码损坏 | seed 安全校验直接拒绝该文件，需求大厅在所有环境为空 | 以合法 UTF-8 重写 4 条需求 |
| canvas seed 复制了 settings 的 `cvs_system_setting` | seed 报 `relation does not exist`，模块初始化失败 | 改为显式 no-op（canvas 无平台设置表） |
| compute seed manifest 缺 `profiles` | 任何 seed 阶段报 `missing field profiles` | 补 canonical 空 profile / localeSets |
| missory legacy seed manifest 结构 | locale 文件引用 common 脚本、checksum 非法 | 迁移到标准 profiles + localeSets 结构 |
| comments 的 AI 评测评论 seed 未挂 profile | 评测线程存在但评论表为空 | 加入 standard profile |
| aiot/skills/webserver 重复 `001_bootstrap.sql` | 同一 seed 执行两次 | 去除重复条目 |
| localeSet checksum 与内容/行尾不一致 | CRLF 检出上生成的 checksum 在 LF 检出上永久不匹配；appbase/iam/membership/payment 等声明值过期 | 统一按 LF 规范化重算；appstore 校验脚本同步规则 |

`seedOnBoot=true` 最终集合（20 个 PG + 1 个 sqlite）：
`appbase`、`appstore`、`cloudrouter`、`community`、`company`、`course`、`documents`、
`drive`、`forum`、`gameengine`、`iam`、`mail`、`mcp`、`membership`、`merchandise`、
`order`、`partner`、`payment`、`settings`、`skills`；sqlite 侧 `agentstudio`。

保持 opt-in（`seedOnBoot=false`）的示例/运营数据模块：
`im`（demo 用户数据）、`comments`（评测 demo 评论）、`github`（demo）、`dezhou`
（demo tenant）、`shop`（注释掉的示例）、`aiot`（dev/pilot 模板）。

## 4. 未修复 / 需人工决策

1. **`sdkwork-games` 与 `sdkwork-gameengine` 重复 moduleId `games`**
   两者都声明 `moduleId=games`、`tablePrefix=game_` 并写同一批 `game_*` 表；同时
   bootstrap 会触发 `game_catalog_uuid_key` 唯一键冲突。`sdkwork-games` 的 README 声明
   核心能力由 `sdkwork-gameengine` 封装、不应重复，gateway 也未把它作为 surface。
   建议：移除 `sdkwork-games/database/`（属破坏性变更，需人工确认）或为其分配独立
   moduleId 与表前缀。**当前回归矩阵中它是唯一确定性失败项。**
2. **course 0001 迁移被就地修改**
   `981dd10` 在 0001 中删除了 5 行 SQL（2026-10-30），而 dev 已按旧 checksum 应用，
   现在 `checksum_mismatch` fail-closed。任何已应用旧 0001 的环境下次 bootstrap 都会失败。
   建议：恢复 0001 原始内容并把变更放进新迁移，或走人工评审的 history 校正流程。
3. **company 仅有 baseline、没有迁移**
   `company_demand_category` / `company_demand_bid` 只在 baseline 中；dev 的旧 schema
   不会被 baseline 更新，seed 报 `relation does not exist`。需补 additive migration
   `0001_company_demand_hall`（schema 变更，需人工确认）。
4. **im demo seed 依赖 IAM demo 用户**
   `002_im_demo_data` 明确要求 owner/guest/alice/bob/grace；`seedOnBoot=false` 下生产
   不受影响，显式 seed 前需运行 `scripts/dev/ensure-im-h5-demo-users.mjs`。
5. **payment 迁移元数据不合规**（`check-database-framework-standard` 唯一失败项）
   0002/0003 缺 reversible/rollback/transactional 等头部字段，属迁移治理问题。
6. **knowledgebase baseline 内含 `CREATE EXTENSION IF NOT EXISTS vector`**
   app 角色无权限；正常路径由 `--provision-databases` 以管理员预建。若运维跳过
   provision，模块初始化会失败。
7. **appstore `015_vertical_evaluation.sql` 仍为孤儿 seed**
   它没有对应 zh/en locale 文案；直接启用会让 9 个应用回退显示 app_key。需先补
   localization 再纳入 profile。
8. **en-US 文案未随启动 seed**
   启动按 `SDKWORK_DATABASE_SEED_LOCALE=zh-CN` 执行；需要英文文案的环境应显式
   `pnpm db:seed --locale en-US`（dev 已双语文案）。

## 5. 关键证据

- 全新 `sdkwork_ai_prod`：`appstore` 235 条 `active/visible` 上架、84 download grant、
  84 install event、235 合规档案、470 区域可用性；`base_country` 252；`company_demand`
  6；`comments_comment` 126；`membership_plan` 6；`commerce_payment_method` 17。
- `ops_database_installation_state`：appstore / base-data / payment / settings / skills /
  company / community / merchandise / forum / mail / mcp / partner / drive / cloudrouter /
  iam / membership / order / course / documents 均为 `seeded`。
- staging/demo 冒烟：5 模块均 `bootstrap complete`，计数与 prod 一致。
- dev 收敛：18/20 模块 `0 pending` 或仅应用变更 seed；失败项为 §4.2/§4.3 的历史 schema 问题。

## 6. 提交

| 仓库 | commit | 内容 |
| --- | --- | --- |
| sdkwork-api-cloud-gateway | `bbeeee1` | Dockerfile surface 键、dev runner 发现、运维文档 |
| sdkwork-appstore | `ae828ae`（及 `745e7bd`） | download grant 状态、seedOnBoot、005/001/015、checksum 规则 |
| sdkwork-appbase | `b577bf9` | seedOnBoot + checksum |
| sdkwork-payment | `4b16f9f` | seedOnBoot + checksum |
| sdkwork-settings | `7f09045` | seedOnBoot |
| sdkwork-skills | `d1757be` | seedOnBoot + 去重 + checksum |
| sdkwork-company | `8ae0e9f` | seedOnBoot + 需求 seed 修复 |
| sdkwork-community | `ebfe7f6` | seedOnBoot |
| sdkwork-merchandise | `07f6d26` | seedOnBoot |
| sdkwork-forum | `b7102d8` | seedOnBoot |
| sdkwork-gameengine | `9f98d3e` | seedOnBoot |
| sdkwork-mail | `f5a14b9` | seedOnBoot |
| sdkwork-mcp | `ea32557` | seedOnBoot |
| sdkwork-partner | `79d377b` | seedOnBoot + checksum |
| sdkwork-drive | `b3acbf7` | seedOnBoot + checksum |
| sdkwork-cloudrouter | `9c1952e` | seedOnBoot + checksum |
| sdkwork-iam | `b1ce1cd` | checksum |
| sdkwork-membership | `03dc762` | checksum |
| sdkwork-canvas | `0bc9c65` | seed no-op |
| sdkwork-compute | `3837a2b` | manifest |
| sdkwork-missory | `2640ecf` | manifest |
| sdkwork-comments | `2fbd963` | profile 挂载 |
| sdkwork-aiot | `bf6ef0e` | 去重 |
| sdkwork-webserver | `5f55ab3` | 去重 |

## 7. 复现

```bash
# 全量回归（WSL，sdkwork-db 已构建）
bash /mnt/d/sdkwork-space/_scratch/db-init-regression/run-regression.sh
# dev 收敛
bash /mnt/d/sdkwork-space/_scratch/db-init-regression/dev-convergence.sh
# staging/demo 冒烟
bash /mnt/d/sdkwork-space/_scratch/db-init-regression/env-smoke.sh
```

脚本与原始产物位于 `D:\sdkwork-space\_scratch\db-init-regression\`（inventory / entries /
runs / module-ledger / table-counts / failures）。

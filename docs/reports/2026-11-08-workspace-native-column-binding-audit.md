# sdkwork-space 跨仓库“文本参数写入原生列”缺陷排查与回归报告（2026-11-08）

## 结论

生产 `POST /app/v3/api/recharges/orders` 50001 的根因不是单点：`sqlx` 把 `String` 绑定成
PostgreSQL `TEXT` 参数，而迁移把列改成了原生 `TIMESTAMPTZ`/`NUMERIC`，SQL 又缺少显式
`CAST`，于是 PostgreSQL 在语句分析阶段直接拒绝。按同一判据扫描 `D:\sdkwork-space` 下
97 个仓库，确认 **11 个仓库存在同类真实缺陷，共 173 处**，已全部修复并逐个 `cargo check`。

| 仓库 | 候选 lead | 确认真实缺陷 | 修复提交 |
|---|---|---|---|
| sdkwork-order | 17 | 33（上一轮已修）+ 回归测试 | `f02c90b`、`50ea9f6` |
| sdkwork-im | 118 | 59 处调用点（tokio-postgres 文本绑定） | `3b7f6941` |
| sdkwork-models | 44 | 45 | `8c03609` |
| sdkwork-cloudrouter | 65 | 14（含 1 处参数索引错位） | `b945119c` |
| sdkwork-knowledgebase | 15 | 6 | `6575606` |
| sdkwork-rtc | 5 | 5 | `aa2fb59` |
| sdkwork-payment | 4 | 3 | `d1b779c` |
| sdkwork-shop | 4 | 3（含 1 处硬编码时间） | `8ea4be8` |
| sdkwork-mail | 32 | 2 | `52d0c09` |
| sdkwork-membership | 2 | 2（含 1 处 BIGINT id 写成字符串） | `242179c` |
| sdkwork-account | 2 | 1（`acct_transfer.amount` 文本写 BIGINT） | `22e6d89` |
| sdkwork-customerservice | 36 | 0（全部是原生 `DateTime<Utc>`） | — |
| sdkwork-manager / portal / agents / generations / partner / image | 20 | 0（原生类型或已带 cast） | — |

## 一、两种驱动行为（为什么“加 CAST”不总是充分）

1. **sqlx（order/payment/shop/account/membership/models/knowledgebase/cloudrouter/mail/rtc）**
   sqlx 为参数声明明确的 PostgreSQL 类型：`&str`/`String` → `TEXT`。因此
   `INSERT ... VALUES ($8)` 命中 `TIMESTAMPTZ` 列、`SET updated_at = $n`、以及
   `WHERE col <= $n` 都会报
   `column "x" is of type timestamp with time zone but expression is of type text`。
   修复是给占位符加最窄的显式转换：`CAST($n AS TIMESTAMPTZ)` / `$n::timestamptz` /
   `CAST($n AS NUMERIC)` / `CAST($n AS DATE)` / `CAST($n AS BIGINT)` / `$n::jsonb`。
2. **tokio-postgres（sdkwork-im social-postgres 适配器）**
   `Parse` 不发送参数 OID，PostgreSQL 按上下文推断为 `timestamptz`，随后客户端
   `to_sql_checked` 拒绝 `&str`（只接受 TEXT/VARCHAR/BPCHAR/NAME/UNKNOWN）。这里仅加 SQL
   转换无效，必须在 Rust 侧把文本转成 `chrono::DateTime<Utc>`（仓库既有
   `postgres_timestamptz` / `optional_postgres_timestamptz` 辅助函数），SQL 再补
   `::timestamptz` 保持一致。

## 二、排查方法

1. **类型目录**：从生产库导出 `schema|table|column|data_type|precision`
   （`sdkwork_ai_prod`，19 984 列）；同时解析各仓库 `database/**/*.sql` 的
   `CREATE TABLE` / `ADD COLUMN` / `ALTER COLUMN ... TYPE`，逐仓库合并（生产目录优先）。
2. **SQL 扫描**：提取 Rust 原始字符串中的 `INSERT INTO` / `UPDATE ... SET` 语句，按列名对齐
   `VALUES` / `SET` 占位符，剥掉已有 `CAST(...)`/`::type` 后仍裸露的 `$n` 即进入复核队列。
3. **逐条判定**：必须读绑定表达式与值声明，只有“文本 Rust 值 + 原生列 + 无转换”才算缺陷；
   `DateTime<Utc>`、`Uuid`、`serde_json::Value`、整数/布尔、`CASE WHEN $n = 'literal'`、
   `IN (...)`、`format!` 拼接 SQL 均为安全。
4. **修复约束**：只改占位符，不动列清单、参数编号、绑定顺序、字面量和业务逻辑；逐 crate
   `cargo check --offline`。

复核队列工具已固化在本仓库：
`node tools/check-native-column-bindings.mjs --workspace D:\sdkwork-space --catalog <prod-columns.txt>`
（`--self-test` 自带断言）。它输出的是**复核队列**而非判决，接入 CI 前需要先评审。

## 三、修复明细

- **sdkwork-order**（上一轮）：充值订单明细 `created_at`、充值支付回执的
  `payment_intent.updated_at` / `payment_attempt.paid_at, updated_at`，以及账户充值、账户值套餐、
  Token 套餐、售后、包裹等 33 处；并新增 PG 回归测试
  `crates/sdkwork-order-repository-sqlx/tests/points_recharge_order_postgres.rs`
  （去掉 CAST 即复现生产原文错误，加回即通过）。
- **sdkwork-im**：`social-postgres` 的 59 处写入（space member / invitation / ban / space / group /
  channel / external connection / shared channel policy / user profile / settings）改为
  `postgres_timestamptz(...)` 绑定 + SQL `::timestamptz`；`Cargo.lock` 顺带补齐 5 条陈旧的
  依赖声明的 lock 条目（`cargo check` 生成）。
- **sdkwork-models**：`admin_ai_resource_store.rs` 的 43 处（资源/分组/成员/层级/rename/sync 的
  `created_at/updated_at/deleted_at`）与 `model_catalog_admin_store.rs` 的 mapping 规则、
  `model_ranking_refresh_store.rs` 的 `win_rate`/`trend_score`（`::numeric`）；另补 2 处同调用路径的遗漏。
- **sdkwork-cloudrouter**：营销套餐/定价计划/费率卡/定价规则的 `requested_at`、`price_amount`
  共 13 处；对账明细 `metadata_json` 的 `$22::jsonb`；并修正
  `upstream_credential_rotation_store.rs` 中 `next_rotate_at <= $3::timestamptz`（`$3` 是
  `account_id: i64`，正确应为 `$4`＝`command.now`）的参数索引错位。
- **sdkwork-knowledgebase**：outbox `next_attempt_at`、两个 PG 测试 fixture 的时间参数；
  另补一处同 seed helper 的遗漏。该模块其余动态 SQL 通过 `SqlTimestampDialect::sql_timestamp_expr`
  注入 `CAST($n AS TIMESTAMPTZ)`，判定安全。
- **sdkwork-rtc**：`rtc_session_token_grant` 两个 `revoked_at` UPDATE 分支 +
  `expire_at/revoked_at/created_at` INSERT（列在生产为 `timestamp without time zone`，
  故用 `CAST(... AS TIMESTAMP)`）。
- **sdkwork-payment**：渠道删除 `deleted_at`、路由规则 `starts_at/ends_at`。
- **sdkwork-shop**：履约单 `created_at/updated_at`；并把 `chrono_like_now()` 从硬编码
  `"2026-06-17 00:00:00"` 改为真实 UTC 时间（否则所有履约单时间戳相同）。
- **sdkwork-mail**：`mail_message.sent_at/received_at`（`::timestamp`，与同文件既有写法一致）。
- **sdkwork-membership**：每日奖励 `reward_date`（`CAST($6 AS DATE)`）；并把
  `membership_daily_reward.id`（生产 BIGINT）从 `"daily-reward-..."` 字符串改为
  `stable_membership_i64_id(...)` 生成 i64——原 INSERT 无论如何都无法成功。
- **sdkwork-account**：`acct_transfer.amount` 以 `CAST($8 AS BIGINT)` 写入。

## 四、回归与验证

| 验证 | 结果 |
|---|---|
| 生产库同一 schema 下 PREPARE 复现（text 参数） | 复现 `column "created_at" is of type timestamp with time zone but expression is of type text` |
| 本地 PG16 + 生产完整迁移链（baseline + 0004–0020）跑 order 回归测试 | 移除 CAST = 红（逐字复现生产错误）/ 加回 = 绿；仓储层 62 个测试全通过 |
| 各修复仓库 `cargo check --offline` | cloudrouter / models / knowledgebase / membership / shop / account / payment / rtc / im 均 `Finished`；mail 为**既有**编译失败（见下） |
| 扫描器 `--self-test` | `self-test OK`；全空间复扫后剩余 179 条均为已判定安全的原生绑定（复核队列） |

## 五、遗留风险（需负责人决策，均未擅自修改）

1. **rtc 读路径解码不匹配**：`session_token_grant.rs` 的 SELECT 直接读
   `expire_at/revoked_at/created_at`，而领域字段是 `String`；列已是原生 `timestamp`，
   sqlx 解码会报类型不兼容。需要改成 `to_char(... AT TIME ZONE 'UTC', ...)` 投影（同仓库其他文件
   已如此）或改领域类型。
2. **im audit-service**：`retention_until: Option<String>` 写入 `im_audit_records.retention_until
   TIMESTAMPTZ`，该 service crate 无 chrono 依赖，修复要加依赖，超出本轮范围。
3. **account `acct_transfer.currency_code`**：baseline 声明 `VARCHAR(16) NOT NULL` 且无默认值，
   而 INSERT 未包含该列；与本次类型问题相邻，需要迁移/代码确认。
4. **mail 独立编译失败**：`sdkwork-communication-mail-repository-sqlx` 在自身 workspace 下
   `cargo check` 因 `sqlx` 未启用 `chrono` feature 而失败（79 个 `NaiveDateTime: Decode` 错误）。
   网关构建因 feature 合并可编译，但该仓库的独立门禁是红的。
5. **models 时间字符串无时区**：`current_timestamp_string()` 产出 `YYYY-MM-DD HH:MM:SS`，
   `CAST(... AS TIMESTAMPTZ)` 依赖会话时区；生产库会话时区已确认为 `Etc/UTC`，当前正确，
   但任何非 UTC 会话会移位。建议后续统一输出 RFC 3339（带 `Z`）。
6. **动态 SQL**：knowledgebase / cloudrouter 有 `format!` 注入时间表达式的写法，当前拼接的是
   `CAST($n AS TIMESTAMPTZ)`，安全但脆弱；一旦有人改成裸 `$n` 会重新引入本类缺陷。
7. **扫描器边界**：不覆盖“原生列 → String 解码”（第 1 项）、部分动态 SQL、以及
   jsonb/uuid/bigint 的文本绑定（后者已单独抽查：仅 account/shop 两处，已修）。

## 六、复现命令

```bash
node tools/check-native-column-bindings.mjs --self-test
node tools/check-native-column-bindings.mjs --workspace D:\sdkwork-space \
     --catalog <prod-columns-tsv>

# order 回归（需要 SDKWORK_DATABASE_TEST_POSTGRES_URL 指向已应用生产迁移链的库）
SDKWORK_DATABASE_TEST_POSTGRES_URL=postgresql://postgres:postgres@127.0.0.1:15432/order_verify \
  cargo test -p sdkwork-order-repository-sqlx --test points_recharge_order_postgres -- --nocapture
```

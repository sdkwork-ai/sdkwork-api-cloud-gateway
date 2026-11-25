# SDKWork 全工作区性能审查报告（架构师 + 性能专家 + 实测）

## 审查范围

| 项 | 数值 |
|---|---|
| 含 Rust 源码的仓库 | **94 个**（`D:\sdkwork-space\sdkwork-*`） |
| Rust 源文件（`src/`，不含 tests/benches） | **7,337 个** |
| Rust 源码行数 | **约 174 万行** |
| 路由清单条目（`HttpRoute::*`，全工作区） | **3,345 条 / 62 个文件** |

上一轮只覆盖了 web-framework + gateway 两个仓库（约 5 万行，占 3%）。本轮扩展到全量。

## 审查方法

1. **确定性静态扫描**：写了一个 Rust 扫描器跑遍全部 94 个仓库，
   覆盖 15 类反模式（正则编译 / HTTP 客户端构造 / body 拷贝 / 阻塞 IO / `block_on` /
   跨 await 持锁 / 无界 spawn / JSON DOM 化 / HeaderMap 拷贝 等），
   输出逐仓库、逐文件、逐行号的清单。**不靠人工猜测找点。**
2. **定向深潜**：按"是否在请求热路径上"排序，对代价最高的三个区域做逐行复核
   （webserver 边缘数据面、agents/LLM 服务、网关+框架），并**独立核实每一条关键结论**。
3. **实测**：隔离微基准 `target/perf-probes`，对可量化的路径给出真实数字，而非估算。

---

## 一、P0 级问题（按请求路径代价排序）

### P0-1 框架路由清单：每请求线性全扫，生产规模 3,345 条

**位置**：`sdkwork-web-framework/crates/sdkwork-web-core/src/route_manifest.rs:49-67`

**证据**（已逐行确认）：
```rust
pub fn match_route(&self, method: &str, path: &str) -> Option<&HttpRoute> {
    let normalized = normalize_path(path);                    // 分配
    if let Some(route) = self.routes.iter()
        .find(|route| is_candidate(route) && !template_has_parameters(route.path)) { ... }
    self.routes.iter().find(|route| is_candidate(route))      // 第二次全量扫描
}
```
`route_path_matches`（`:388-398`）对**每条路由**都调用两次 `path_segments`，
每次 `normalize_path` 用 `format!` 分配、再逐段 `to_owned()` —— 每条路由约 2 次 String + N 次分段分配。

**生产规模（本轮实测）**：全工作区 62 个 manifest 共 **3,345 条路由**。单仓库最大的：
`sdkwork-cloudrouter` 421、`sdkwork-iam` 263、`sdkwork-appstore` 244、`sdkwork-im` 219。
网关的 `composed.route_manifest`（`runtime.rs:387`）由 `try_merge_mounts` 合并全部已挂载
surface 的清单，按 `platform-foundation-suite-container`（56 个 surface）组合，
**是数百到上千量的合并清单**（不代表全部 3,345 条，但量级与之同阶）。

**每请求重复调用**：`interceptors.rs:156` 与 `:168` 是紧邻的两次**完全相同**调用；
加上 `:149`（`resolve_public_path` 内部）、`:460`、`policies.rs:33-35`、`:1014`。
另按架构评审，ManifestAuthorizationPolicy 在授权阶段还会再触发一次。

**实测**（`target/perf-probes --bin routes`，镜像上述真实代码形状）：
```
 100 routes  CURRENT miss (2 full scans)     121.57 us/op   INDEXED 0.17 us/op
 500 routes  CURRENT miss (2 full scans)     618.09 us/op   INDEXED 0.14 us/op
1000 routes  CURRENT miss (2 full scans)   1188.07 us/op   INDEXED 0.15 us/op
1000 routes  CURRENT hit  (param route)    1230.27 us/op   INDEXED 7.95 us/op
```
**未命中比命中更贵**（必须跑完两轮）。按 1000 条、每请求 6 次计，单请求约 **7 ms 纯 CPU**。

**修复**（按收益排序）：
1. **最省事、先做**：`interceptors.rs` 里同一 `(method, path)` 只解析一次，
   存进 `WebCallState`，`:168`/`:460`/`:1014`/authorize 全部复用 —— 直接去掉 4 次冗余。
2. **根治**：`HttpRouteManifest` 构造期建索引（`HashMap<(method, literal_path), idx>` +
   参数路由按段数分桶），并在 `try_merge_mounts`（`:242-281`）合并后重建索引。
3. 消除 `route_path_matches` 的逐段分配，改用惰性 `split('/')` 迭代器比对。

---

### P0-2 webserver 边缘：正则每个请求现编译（19–46 µs ×N）

这是**全网最大流量入口**（`sdkwork-webserver`，23.7 万行）的请求路径。

**好消息先说**：路由选择本身**设计正确**——`config/compiled.rs:856-868` 的正则编译发生在
`CompiledVirtualHost::compile`（配置编译期，一次性），`select_route`（`:890-925`）使用
exact 哈希表 + 前缀树 + 预编译正则列表。这一层没有问题。

**问题在路由选中之后**：

| 位置 | 问题 |
|---|---|
| `data_plane/rewrite_phase.rs:42-51` | `location_captures` 每请求 `Regex::new(&compiled)`；调用点 `handler.rs:907` **每个走 rewrite 的请求都执行** |
| `data_plane/rewrite_phase.rs:129` | `RewritePhaseOpConfig::Rewrite` 每请求编译 |
| `data_plane/rewrite_phase.rs:214` | `if` 指令的正则条件，**每次求值都编译** |
| `data_plane/proxy_response_policy.rs:374` | 遍历响应策略规则，**每条规则每次编译** |
| `data_plane/cgi_upstream.rs:350` | `split_path_info` 每请求编译 |

**实测**（`--bin wsregex`，真实 `regex` crate）：
```
^/api/(?<svc>[a-z0-9-]+)/v[0-9]+/items/(?<id>[0-9]+)$
  CURRENT Regex::new per request (+match)    18.99 us/op   FIXED precompiled 0.11 us/op
^/static/(.*)$
  CURRENT Regex::new per request (+match)    37.83 us/op   FIXED precompiled 0.10 us/op
(?i)^/blog/... 带忽略大小写
  CURRENT Regex::new per request (+match)    45.69 us/op   FIXED precompiled 0.11 us/op
```
**每次 19–46 µs，预编译后 0.1 µs，相差 170–380 倍。** 一个请求只要命中 3 条正则规则
（location capture + if + rewrite），就白白烧掉 **60–140 µs CPU**；若响应策略有 N 条规则，
按 N 线性放大。

**修复**：在配置编译期把正则编译成 `Arc<Regex>` 存进 `RouteConfig` / 规则结构体
（与已有的 `CompiledVirtualHost` 同思路），请求期只做 `is_match` / `captures`。
这属于**纯收益、零行为变更**的改动。

---

### P0-3 网关评论代理：每请求新建客户端 + 全局零超时 + 绕过治理

**位置**：`sdkwork-api-cloud-gateway/src/comments_proxy.rs:39`、调用点 `runtime.rs:1007`

```rust
:39   let client = reqwest::Client::new();     // 每请求新建，连接池无法复用
:55   match upstream.send().await { ... }     // 全模块 timeout 出现次数 = 0
```
调用位于 `registry.resolve()`（`:1010`）、`policy::evaluate_request()`（`:1018`）、
traffic 准入**之前** → 这两个前缀完全跳过策略与限流。
reqwest 默认**总超时与连接超时均为无限**，上游挂起会永久占用任务。

**修复**：`OnceLock<Client>` 单例 + 显式 `.connect_timeout(3s).timeout(30s)`；
把评论前缀判定移到 traffic/policy 之后，或为其单独挂限流。
顺带：`is_comments_path`（`:18-22`）在 `any` 闭包里 `format!`，每请求分配——
实测 **miss 路径 145.3 ns → 0.4 ns（~360×）**，改为字节前缀比较即可。

---

### P0-4 agents 嵌入式 turn 传输：完整缓冲整个 SSE 流

**位置**：`sdkwork-agents/crates/sdkwork-agents-tool-cloudrouter/src/turn_transport.rs:468`

```rust
let body = axum::body::to_bytes(response.into_body(), MAX_EMBEDDED_BODY_BYTES).await?;
:492  const MAX_EMBEDDED_BODY_BYTES: usize = 64 * 1024 * 1024;   // 64 MiB
```
一次 in-process dispatch 会把**整个流式响应**读成 `Vec<u8>`，再 `accumulate_stream_body`
二次解析，之后才发 delta。

**为什么是 P0**：这直接摧毁了 LLM 的首 token 时延（TTFT）——
原本 300 ms 出首字，变成「等完整答案 xxx 秒 + 一次性吐出」；峰值内存
**每条并发 turn 最多 64 MiB**，按 agents 的 128 并发 worker 上限最坏可达 **8 GiB**；
超过 64 MiB 的答案直接失败。

**对比**：同仓库另外三条流式路径是**正确**的——
`http.rs:14870-14909`（mpsc 128 分块、15 s SSE 心跳，内存 O(128 块)）、
`chat_stream.rs:841-860`（8 KiB 增量解析）、
`cloudrouter invocation_stream.rs:76-140`（`mpsc::channel(1)` 真背压，「without buffering it」）。
说明团队**有正确的范式**，只是嵌入式分支没走它。

**修复**：把该分支改为 `response.into_body().into_data_stream()`，
用增量 SSE 解码器逐帧转成 delta；底线是把 64 MiB 上限降到几 MiB 并显式报错。

---

### P0-5 agents MCP 工具调用：每次调用新建 blocking 客户端

**位置**：`sdkwork-agents/.../tool_calling.rs:858`（位于 `fn execute`，`fn` 而非 `async fn`）

```rust
let client = reqwest::blocking::Client::builder().timeout(...).build();
```
**同仓库的其他客户端都是正确的**：`chat_stream.rs:685-695`、`outbox_dispatch.rs:74-84`
用 `OnceLock` + `pool_max_idle_per_host(8)` 建一次复用。
只有这里每次工具调用新建——`reqwest::blocking::Client` 每个实例自带内部运行时线程，
**每次工具调用新开 TLS 握手（+50~200 ms）、新增 2 个 OS 线程**。

**修复**：提升为 `static OnceLock<Client>`（按 timeout 分键），或改用异步 `reqwest::Client`。

---

## 二、P1 级问题

| # | 位置 | 问题 | 实测/量化 |
|---|---|---|---|
| P1-6 | 框架 `stores.rs:124-160` | `MemoryRateLimitStore` **无过期、无上限**，每请求 `entry().or_insert_with`，全文件无移除逻辑。而同文件幂等存储有 10k 条 + 256 MiB 双上限 | 每个不同 URL 永久占一个键 |
| P1-7 | 框架 `api_chain.rs:553-575` + `:639-649` | 限流 key 5~7 次分配；`credentials_fingerprint()` 为造 6 个 bool 的字符串再哈希 | **346 ns → 47 ns（~7×）**；10 万键 `String` 21.8 ms vs `u64` 3.6 ms（~6×） |
| P1-8 | `sdkwork-local-router/.../interceptor.rs:811-841` | `pattern_matches` 每次匹配都 `Regex::new`；由 `pattern_list_matches` 按规则列表逐条调用，挂在 `on_before_transform`/`on_before_forward`（**每次 LLM 调用**）与 `account_blocked`（`retain` 遍历候选账号）上 | 每规则每次 19–46 µs × 账号数 |
| P1-9 | `sdkwork-agents/.../application.rs:9450-9472, 364-381` | `heartbeat_turn_lease` 用 `std::thread::scope` **每条 turn 起一个 OS 线程**，内部 `std::thread::sleep(1s)` 轮询 | 线程数 = 并发 turn 数（≤128）；join 再给每条 turn 加最多 1 s |
| P1-10 | `sdkwork-agents/.../http.rs:14882` | turn 的 `JoinHandle` **在客户端断开时不被 abort**，只 drop sender | 30 s 的 turn 白跑，占着 provider 许可与额度 |
| P1-11 | `sdkwork-knowledgebase/.../async_bridge.rs:16,19` | KB 桥接队列容量 64、超时 30 s，而 agents worker 上限 128 | 2:1 超订，突发时大量 `QueueSaturated` |
| P1-12 | `sdkwork-knowledgebase/.../agent_chat_runtime.rs:113-335` | 每个 KB 客户端方法（含 `retrieve`）走 `block_on_async` | 检索热路径 +1 次队列跳转，最坏 30 s |
| P1-13 | 网关 `metrics.rs:48-56` | 6 张 `BTreeMap` 无基数上限、无驱逐；渲染时合并全部分片 | 框架侧有 4096/128 上限 + 丢弃计数，网关侧没有 |
| P1-14 | agents `generations_tool_port.rs:13,21` | turn 工具路径用 `reqwest::blocking`，超时 **300 s** | 挂起的调用会把 blocking worker + provider 许可钉 5 分钟 |

---

## 三、P2 级问题（批量清理）

| # | 位置 | 问题 | 建议 |
|---|---|---|---|
| P2-15 | 网关 `traffic.rs:284-289` | 满 100k 时**持全局锁** `retain` 全表，且每请求重跑 | 后台增量清理，锁外执行 |
| P2-16 | 框架 `security.rs:1079-1086, 1120-1127` | 每响应重建 `access-control-allow-methods` | 配置期预计算 `HeaderValue` |
| P2-17 | 网关 `runtime.rs:772, 1004, 1307` | 每请求 `HeaderMap::clone()`（最坏 3 次） | 传 `&HeaderMap` |
| P2-18 | 网关 `policy.rs:143-146`、`traffic.rs:400` | 比较闭包内 `format!("{}/", prefix)` | 反序列化期预计算 |
| P2-19 | `cloudrouter/.../passthrough.rs:1672-1681` | 每个适配器请求整体解析成 `serde_json::Value` | 只取路由字段（`RawValue`/类型化结构） |
| P2-20 | `agents/.../chat_stream.rs:777-792` | 每条消息 `to_value` 再 `json!` 包一层，整段对话 DOM 复制两次 | 直接 derive `Serialize` |
| P2-21 | `memory/.../tenant_quota.rs:47-51` | 全局 `std::sync::Mutex<HashMap>`，每次配额解析都要拿 | 分片或 `dashmap`（未跨 await，无死锁） |
| P2-22 | `agents/.../turn_runtime.rs:426` | 超时后仍持许可再阻塞 5 s 做取消 | 释放许可后再取消 |

---

## 四、经核实**不是问题**的点（重要，避免重复排查）

这一节是本轮最有价值的部分之一——多条「看起来有问题」的代码经逐行核实是干净的设计：

- **路由选择（webserver）**：`config/compiled.rs:856-868` 的正则编译在 `compile` 期一次性完成，
  `select_route` 用哈希表 + 前缀树 + 预编译正则。**设计正确**，不要误报。
- **memory 敏感词正则**：`sensitive_content.rs:9-40` 是 `OnceLock<Vec<Regex>>`，
  进程内只编译一次。扫描器命中的 `filter_map(|p| Regex::new(p))` 位于 `get_or_init` 内部——
  **不是每请求重建**。（子代理主动推翻了我基于扫描的初判，已复核确认。）
- **agents 的 65 处 `block_on`**：绝大多数位于 `spawn_blocking` worker 内
  （`http.rs:13684`），并发由 128 许可 + 32 provider 许可双重限制，**按设计安全**。
  真正的热点是 P0-5 那一处客户端新建，不是 `block_on` 本身。
- **LLM 流式**：除 P0-4 嵌入式分支外，三条主流路径都是真流式（增量解析 + 背压），
  **没有**"全量缓冲再转发"的普遍性问题。
- **跨 await 持锁**：四个被深审仓库**零命中**。
- **无界 spawn**：agents 的 `spawn_blocking` 由信号量约束；
  cloudrouter 每流 2 个 task 均通过 `track_background_task()` 注册并在 shutdown 排空。
- **KB 检索阻塞**：`embedding_retrieval_backend.rs:31-46` 用 `run_bounded_blocking` 包裹，
  **不是**裸 `std::fs`。
- **网关路由注册表** `registry/src/lib.rs:132-146`：**已按首段建索引**，不是线性扫描
  （被线性扫描的是框架那套 `HttpRouteManifest`，两者是独立实现，勿混淆）。
- **框架 metrics**：4096/128 上限 + 原子丢弃计数，已保护。

---

## 五、静态扫描全景（供持续治理）

全工作区 7,337 个文件的命中统计（高中危信号）：

| 反模式 | 总数 | 最集中的仓库 |
|---|---|---|
| `blocking_io_or_sleep` | 446 | webserver 267、database 22、iam 21、drive 21 |
| `json_to_value_dom` | 298 | agents 35、knowledgebase 20、memory 20 |
| `tokio::spawn` | 256 | webserver 112、cloudrouter 23、knowledgebase 20 |
| `body_clone` | 123 | web-framework 17、webserver 13、knowledgebase 11、payment 11 |
| `block_on_in_async` | 160 | agents 65、birdcoder 19、aiot 14 |
| `process_spawn` | 296 | order 98、partner 55、birdcoder 57 |
| `headermap_clone` | 76 | webserver 51 |
| `regex_compile` | 68 | webserver 24、im 22、local-router 4、plugins 6 |
| `http_client_construct` | 64 | 27 个仓库 |

**需要说明的是**：这些计数是**候选清单，不是缺陷清单**。本轮抽检证明其假阳性率相当高
（如 `blocking_io_or_sleep` 在 webserver 的 267 条绝大多数是 TLS/配置/启动代码；
`process_spawn` 在 order/partner 多为构建或运维命令）。正确用法是：
**用扫描器定位候选 → 用「是否在请求路径上」过滤 → 再人工确认**。

---

## 六、建议的落地顺序

**第一批（低风险、纯收益、当天可做）**
1. webserver 正则预编译（P0-2）—— 收益最大且零行为变更
2. 框架 `interceptors.rs` 去重路由解析 4 次（P0-1 第 1 条）
3. 网关评论代理客户端单例 + 超时 + `is_comments_path` 去分配（P0-3）
4. agents MCP 客户端 `OnceLock`（P0-5）

**第二批（本迭代，根治性）**
5. `HttpRouteManifest` 构造期索引（P0-1 第 2、3 条）
6. agents 嵌入式 turn 改真流式（P0-4）
7. 框架限流存储加驱逐 + `u64` 键（P1-6/P1-7）
8. local-router `pattern_matches` 预编译 + 规则预编译进配置（P1-8）

**第三批（治理性）**
9. agents heartbeat 线程改 `tokio::time::interval`、断连取消 turn（P1-9/P1-10）
10. KB 桥接容量与 agents worker 上限对齐（P1-11）
11. 网关 metrics 照搬框架侧基数上限（P1-13）
12. P2 批量清理

**验证方式**：两个仓库**至今没有任何 benchmark**——这正是所有结论此前只能靠代码形状推断的原因。
建议为 `HttpRouteManifest::match_route`、webserver 的 `location_captures`/`select_route`、
`is_comments_path` 补 criterion 基准，并把这个扫描器纳入 CI 作为回归门禁。

---

## 七、本次审查的边界

- 扫描覆盖 **94 个仓库全部 `src/`**，但**深潜只做了三个区域**（webserver、agents 集群、
  网关+框架）。其余仓库（drive / iam / order / appstore / im / deployments 等）
  只有扫描级覆盖，**未做逐行复核**——它们的 P1/P2 问题需要下一轮针对性地看。
- 所有延迟数字来自**隔离微基准**，复现的是审计代码的精确形状（同算法、同分配），
  不是端到端压测；绝对值随机器变化，**倍数关系**才是结论依据。
- P0-1 的「每请求 6 次」来自代码中的调用点计数，生产实际命中次数
  （尤其 authorize 阶段内部）未做运行时测量。
- 并发上限（128 / 32 / 64）均为代码默认值，可通过环境变量覆盖，未核对各环境部署 TOML。
- webserver 深潜子代理中途失败，该部分由我直接逐行复核补完；因此 webserver 的
  视角偏"regex + 配置编译"，其代理/流式/连接池维度**未做同等深度的系统审查**，
  属后续补强项。

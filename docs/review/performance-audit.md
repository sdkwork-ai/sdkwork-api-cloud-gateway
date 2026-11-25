# SDKWork 性能审查报告（架构师 + 实测双视角）

审查对象：
- **框架**：`D:\sdkwork-space\sdkwork-web-framework\crates`（15 个 crate，约 29,600 行）
- **网关**：`D:\sdkwork-space\sdkwork-api-cloud-gateway\crates`（4 个 crate，约 13,500 行）

方法：三名独立审查者（结构审查 / 对抗式核查 / 实测）交叉验证。所有行号均已逐条打开源码确认；
关键结论配隔离微基准实测（`target/perf-probes`，不依赖仓库内部夹具）。
两个仓库 `git status` 均为空，审查期间未改动任何源码。

---

## 一、结论摘要

按"每个请求都会付的代价"排序：

| # | 问题 | 位置 | 性质 | 实测/量化 |
|---|---|---|---|---|
| P0-1 | 路由清单每请求线性全量扫描，且**同一请求内重复 5~11 次** | 框架 `route_manifest.rs:49-67`、`interceptors.rs:156/168/460` | 吞吐上限 | 1000 路由 **1.19 ms/次** → 索引后 0.15 µs（**~7900×**） |
| P0-2 | JSON 请求体被**全量缓冲 + 解析成 `Value` 两次**（WAF 一次 + 框架守卫一次） | 网关 `security.rs:596-617`、框架 `client_context_guard.rs:148-188` | 内存/延迟尖峰 | 每个写请求多一次全量解析 + 一次整体拷贝 |
| P0-3 | 评论代理 `reqwest::Client::new()` **每请求新建、全模块零超时**，且绕过路由/策略/限流 | 网关 `comments_proxy.rs:39`、`runtime.rs:1007` | 可用性 | 无连接复用；上游挂起则永久占用任务 |
| P1-4 | 评论路径判断在**每次请求**做 `format!` 分配 | 网关 `comments_proxy.rs:18-22` | 全员税 | 145 ns → 0.4 ns（**~360×**） |
| P1-5 | 框架 `MemoryRateLimitStore` **无过期、无上限** | 框架 `stores.rs:124-160` | 内存泄漏 | 每个不同 URL 永久占一个 key |
| P1-6 | 框架限流 key 构造 5~7 次分配、多次哈希 | 框架 `api_chain.rs:553-575, 639-649` | 全员税 | 346 ns → 47 ns（**~7×**） |
| P1-7 | 网关 metrics 6 张 `BTreeMap` 无基数上限，反向合并渲染 | 网关 `metrics.rs:48-56, 227-231` | 内存/抓取抖动 | 框架版有 4096 上限，网关版没有 |
| P2-8 | 每响应重建 `access-control-allow-methods` 头 | 框架 `security.rs:1079-1086, 1120-1127` | 小 | 可静态化 |
| P2-9 | 代理与兜底路径每请求 `HeaderMap::clone()`，最坏 3 次 | 网关 `runtime.rs:772, 1004, 1307` | 小-中 | 可传引用 |
| P2-10 | 满容量时持全局锁 `retain` 10 万条目，**每请求重跑** | 网关 `traffic.rs:284-289` | 延迟尖峰 | 满负载时雪崩放大 |
| P2-11 | 路由/流量规则里在比较闭包内 `format!` 前缀 | 网关 `policy.rs:143-146`、`traffic.rs:400` | 小 | 可配置期预计算 |

**最值得先修的三个**：P0-1（纯 CPU 大项，随路由数线性恶化）、P0-3（唯一的可用性风险）、P1-4（几乎零成本即可消除的全员税）。

---

## 二、逐项分析与优化建议

### P0-1 路由匹配：每请求 1.19 ms 纯 CPU，且重复执行

**证据**（已逐行确认）：

```rust
// sdkwork-web-core/src/route_manifest.rs:49-67
pub fn match_route(&self, method: &str, path: &str) -> Option<&HttpRoute> {
    let normalized = normalize_path(path);                      // 每次分配
    if let Some(route) = self.routes.iter()
        .find(|route| is_candidate(route) && !template_has_parameters(route.path)) { ... }
    self.routes.iter().find(|route| is_candidate(route))        // 第二次全量扫描
}
```

`route_path_matches`（`:388-398`）对每条路由都调用两次 `path_segments`，每次
`normalize_path`（`format!`）再逐段 `to_owned()` —— 即**每条路由约 2 次 String + N 次分段 String 分配**。
未命中时必然跑完两轮扫描，所以**未命中比命中更贵**。

同一请求内的重复调用（均使用完全相同的 `(method, path)`）：
- `interceptors.rs:149` `resolve_public_path` → 内部匹配一次
- `interceptors.rs:156` SurfaceClassification 匹配一次
- `interceptors.rs:168` **紧接 156 行，完全重复**，只为调 `validate_route_auth_credentials`
- `interceptors.rs:460` TenantIsolation 再一次
- `policies.rs:33-35` `ManifestAuthorizationPolicy::authorize` 再一次
- `interceptors.rs:1014` 再一次

**实测**（`target/perf-probes`，镜像上述真实代码形状）：

```
 100 routes  CURRENT  miss (2 full scans)     121.57 us/op   INDEXED 0.17 us/op
 500 routes  CURRENT  miss (2 full scans)     618.09 us/op   INDEXED 0.14 us/op
1000 routes  CURRENT  miss (2 full scans)    1188.07 us/op   INDEXED 0.15 us/op
1000 routes  CURRENT  hit  (param route)     1230.27 us/op   INDEXED 7.95 us/op
```

生产镜像按 `platform-foundation-suite-container` 构建（`Dockerfile:3`、
`scripts/package-server.mjs:50`），组合 **56 个 foundation surface**；抽样各 workspace 的路由清单为
appbase 39 / order 67 / payment 19 / shop 45 …，合计量级在 **数百到上千条**。
按 1000 条计，单次匹配约 1.19 ms，每请求重复 6 次 → **约 7 ms 纯 CPU/请求**，
在 4096 并发上限（production.toml:100）下足以打满 CPU。

**优化建议**（按收益排序，可独立落地）：

1. **在 `HttpRouteManifest` 构造期建索引**，不要把扫描留到请求期：
   ```rust
   pub struct HttpRouteManifest {
       routes: Arc<[HttpRoute]>,
       literal: HashMap<(HttpMethod, Box<str>), u32>,   // 字面量路由 O(1)
       param_by_segment_count: HashMap<u8, Vec<u32>>,   // 参数路由按段数分桶
   }
   ```
   `match_route` 先用已 `normalize` 的 path 查 `literal`，再只在同段数桶内匹配参数路由。
   字面量优先于参数的语义天然保留。记得 `try_merge_mounts`（`:242-281`）在合并后重建索引。
2. **消除 `route_path_matches` 的逐段分配**：改为 `path.split('/').filter(|s| !s.is_empty())`
   惰性迭代器直接比对，模板侧在构造期预存 `Vec<&str>`/段数，不再每条路由都 `path_segments`。
3. **同一请求只解析一次**：在 `SurfaceClassification` 分支算一次
   `let matched = manifest.match_route(&state.method, &state.path);`，
   后续 `:168`、`:460`、`:1014`、`authorize` 全部复用结果（结果可存 `WebCallState`）。
4. `normalize_path` 对已是标准形式的 path 直接借用（`Cow<str>`），避免每请求再分配一次。

> 第 3 条改动最小、风险最低，应**先做**；第 1、2 条是根治。

---

### P0-2 JSON 请求体被完整缓冲并解析两次

**证据**：
- 网关 WAF，production 已开（`etc/sdkwork-api-cloud-gateway.production.toml:85`
  `inspectJsonBody = true`，`mode = "enforce"`）：
  `security.rs:596-617` → `to_bytes(body, max_body_bytes)` → 复制 body → `serde_json::from_slice::<Value>`（`:614`）→ 递归遍历。
  上限 `maxBodyBytes = 33554432`（同文件 `:45`，**32 MiB**）。
- 框架守卫 `client_context_guard.rs:135-188`：对 JSON 请求**再次** `to_bytes`（`:149`）、
  `serde_json::from_slice::<Value>`（`:166` / `:183`），再 `Body::from(bytes)` 复原（`:187`）。
  默认 `json_inspect_limit` 为 16 MiB（`interceptors.rs:306-312`）。
- 二者都在中间件/拦截器链上，因此**每个 JSON 写请求都要完整读入内存、解析成 DOM、丢弃**，共两次。

**影响**：大 body 请求的 p99 与内存峰值翻倍；`Value` 的 DOM 分配量约为原文的数倍。
嵌套深的对象还有栈深风险。

**优化建议**：
1. **廉价拒绝前置**：把 `Content-Length` 声明检查（框架 `:141-147` 已有，但不依赖它时无保护）
   和 WAF 的声明长度检查移到**任何 buffering 之前**，超限直接拒绝，不读流。
2. **流式扫描替代 DOM 解析**：两者都只需要"是否存在某个禁止的键"。
   用 `serde_json` 的 `StreamDeserializer` / 逐 token 扫描，在遇到目标键时立刻返回，
   内存 O(1)，且大多数请求在首个对象开头就能确定无命中。
3. **消除重复**：既然网关 WAF 已强制扫描整个 body，框架侧守卫应改为
   "仅当上游未启用 WAF 时才执行"（配置开关），避免同进程内同一 body 被判两次。
4. **不要重新分配 body**：`Body::from(bytes)` 是移动而非拷贝（已确认），
   但 WAF 处存在一次显式拷贝（`security.rs:617`），应从源头消除。

---

### P0-3 评论代理：每请求新建客户端、零超时、绕过治理链

**证据**（`comments_proxy.rs`，全文 89 行）：
```rust
:39   let client = reqwest::Client::new();          // 每请求新建，连接池无法复用
:55   match upstream.send().await { ... }          // 全模块无任何 timeout
:8    COMMENTS_PATH_PREFIXES = ["/app/v3/api/comments", "/app/v3/api/engagement"]
```
调用点在 `runtime.rs:1007`，位于 `gateway_fallback` 中，且在
`registry.resolve()`（`:1010`）、`policy::evaluate_request()`（`:1018`）、traffic 准入**之前**。

**影响**：
- reqwest 默认**总超时与连接超时均为无限**；上游挂起会永久占用该任务，逐步耗尽 worker。
- 每请求新建 `Client` 意味着新建连接池、无 keep-alive，TLS/TCP 握手无法复用。
- 这两个前缀**完全跳过**策略校验与限流，是治理面上的缺口。

**优化建议**：
1. 将 `Client` 提升为进程内单例（`OnceLock<reqwest::Client>` 或放入 `GatewayState`），
   构造时设置 `.connect_timeout(Duration::from_secs(3))` 与 `.timeout(Duration::from_secs(30))`
   （按上游实际 SLA 调），并设置 `.pool_max_idle_per_host(...)`。
2. 为 `send()` 外包一层 `tokio::time::timeout` 做双保险。
3. 把评论前缀的判定**移到 traffic/policy 之后**，或至少为其单独挂载限流规则，
   避免成为绕过治理的后门。
4. `is_comments_path` 的 `format!` 见 P1-4（同一函数）。

---

### P1-4 `is_comments_path` 每次请求做 `format!`（全员税）

**证据**（`comments_proxy.rs:18-22`）：
```rust
COMMENTS_PATH_PREFIXES.iter()
    .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
```
`format!` 在 `any` 的闭包内，**每个前缀每次都要堆分配**；且短路失效时 miss 路径要走完所有前缀。
该函数在 `gateway_fallback` 中每个请求都执行（含所有未命中路由的探测流量）。

**实测**：
```
is_comments_path CURRENT hit        74.5 ns/op   FIXED 0.5 ns/op
is_comments_path CURRENT miss      145.3 ns/op   FIXED 0.4 ns/op     （~360×）
```

**优化建议**（零风险、纯收益）：
```rust
fn is_comments_path(path: &str) -> bool {
    let p = path.as_bytes();
    COMMENTS_PATH_PREFIXES.iter().any(|prefix| {
        let b = prefix.as_bytes();
        p.len() > b.len() && p.starts_with(b) && p[b.len()] == b'/'
    })
}
```
若坚持字符串语义，可把 `format!("{prefix}/")` 在 `const` 或 `OnceLock` 中预计算。

---

### P1-5 框架默认限流存储无过期、无上限

**证据**（`stores.rs:118-160`）：`buckets: Mutex<HashMap<String, MemoryRateLimitBucket>>`，
`check_and_record` 用 `entry(key.to_owned()).or_insert_with(...)` 插入，
**全文件没有任何移除逻辑**。`is_in_process()` 返回 `true`，是默认的内存实现（`api_chain.rs:201`）。
key 见下条，包含路径哈希——**每个不同的 URL 都会永久新增一个键**。

对比：同文件 `MemoryIdempotencyStore`（`:171-175`）已实现 10,000 条 + 256 MiB 双上限驱逐，
`MemoryConcurrentAdmissionStore`（`:102-111`）计数归零即删。**限流存储是唯一漏网的**。
网关侧对应实现 `traffic.rs:108-112` 也有 600 s 空闲 TTL + 100k 硬上限。

**影响**：长运行进程的内存单调增长；扫描型流量（大量不同路径）可被用来放大内存。

**优化建议**：
1. 复用 `MemoryIdempotencyStore::evict_over_budget` 的思路：加 `MEMORY_RATE_LIMIT_MAX_ENTRIES`
   （如 100k）与"窗口结束后即删"的惰性清理。
2. 在窗口过期分支（`:148-151`）已有"重置计数"的位置，顺手做惰性驱逐：
   超过上限时按 `window_start` 扫描删除已过期桶（增量、摊销，不要每请求全扫）。
3. 键改为 `u64`（见下条），顺带把每请求的 String 分配也去掉。

---

### P1-6 限流 key 构造 5~7 次分配

**证据**（`api_chain.rs:553-575` + `:639-649`）：
```rust
let path_hash = hash_key_material(&self.path);            // 1 分配 + 1 次全路径哈希
format!("tenant:{}:path:{...}", hash_key_material(principal.tenant_id()), ...)  // 再 2 分配
// 凭据分支还要：hash_key_material(&self.credentials_fingerprint())
//   → 先 format! 造 6 个 bool 的字符串，再对字符串哈希一次（纯浪费）
```
`credentials_fingerprint()`（`:639-649`）只编码**六个 bool 的存在性**，
却为此做了一次堆分配再生效于哈希。

**实测**：
```
rate_limit_key CURRENT (2 hashes + 4 allocs)   346.4 ns/op
rate_limit_key FIXED   (u64 key, 1 buf)         47.1 ns/op        （~7×）
```

**优化建议**：
1. path 哈希结果缓存在 `WebCallState`（每请求只算一次；`apply_rate_limit` 里 `:1235` 还会再算一次）。
2. 用 `u8` 位掩码替代 `format!` 指纹：
   ```rust
   let mask = u8::from(a) | u8::from(t) << 1 | ...;
   ```
   直接喂进同一个哈希器，**省掉一次分配 + 一次哈希**。
3. 存储键直接用 `u64`（`HashMap<u64, Bucket>`），仅在日志/渲染时才格式化：
   实测 10 万键插入+查找，`String` 键 21.8 ms vs `u64` 键 3.6 ms（**~6×**）。

> 附带发现（**需产品确认**，非性能问题）：凭据分支只用六个 bool 的**形状**做指纹，
> 意味着所有"同样带 API Key"的调用方会**共享同一个限流桶**。代码注释暗示这是有意为之，
> 但请架构侧确认这是期望的契约，否则应改用真实凭据哈希。

---

### P1-7 网关 metrics 无基数上限

**证据**（`metrics.rs:48-56`）：六张 `BTreeMap`，`record_*`（`:125/147/167/186/205/224`）
各自 `entry(labels).or_insert(0)` 增长，**没有数量上限、没有驱逐**；
渲染时 `:227-231` 反向合并所有分片。
框架侧的对应实现**已经**有保护：`DEFAULT_MAX_LABELED_REQUEST_SERIES = 4_096`、
`MAX_METRIC_SERIES_KEY_BYTES = 2_048` 与原子丢弃计数（框架 `metrics.rs:9-13`）。

**评估**：当前标签值集合本身有界（`route_target` ∈ gateway|dependency|unmatched，
安全事件标签是字面量），所以短期内不会爆炸；但这是**依赖上游约束的隐式安全**，
一旦有人把 status 或 path 维度加进标签就会失控。另外 `record` 用 `std::sync::Mutex`，
抓取期间请求线程会短暂阻塞（未跨 await，无死锁）。

**优化建议**：直接照抄框架侧的实现——给每张 map 加上限 + 超限计数，
超出后归入 `__other__` 标签，并暴露 `metrics_series_dropped_total`。

---

### P2-8 / P2-9 / P2-10 / P2-11（小项，批量清理）

| 项 | 位置 | 建议 |
|---|---|---|
| P2-8 | 框架 `security.rs:1079-1086, 1120-1127` | `.collect::<Vec<_>>().join(", ")` 每响应重建 → 配置期预计算为 `HeaderValue` 常量 |
| P2-9 | 网关 `runtime.rs:772, 1004, 1307` | 每请求 `HeaderMap::clone()`（最坏 3 次）→ 改为 `&HeaderMap` 传引用；仅在确实需要持有所有权处 clone |
| P2-10 | 网关 `traffic.rs:284-289` | 满 100k 时持**全局锁** `retain` 全表，且**每请求重跑** → 改为后台增量清理（按 `last_seen` 分片、每次限 N 条），锁外执行 |
| P2-11 | 网关 `policy.rs:143-146`、`traffic.rs:400` | `format!("{}/", prefix.trim_end_matches('/'))` 在比较闭包内 → 反序列化时预计算规范前缀 |

---

## 三、明确排除的误报（已验证为干净）

这些是同类审查中**经常被误报**的点，本次逐处读过源码，确认**不是问题**，后续不必重复排查：

- **std `Mutex`/`RwLock` 跨 await 持有**：干净。框架 `policy_cache.rs:38-65`、网关
  `cors_dynamic.rs:222/263-267`、`traffic.rs:248-250` 的守卫都在 await 前结束作用域。
- **异步路径中的阻塞调用**：干净。`std::fs` 仅在构建与启动代码；数据库全部 `sqlx` 异步，
  `database_provisioning.rs:1114-1118` 每条语句都包了 `tokio::time::timeout`；
  所有 `block_on` 都在 `#[cfg(test)]`。
- **每请求编译正则**：干净。两个目标中 `Regex::new` 调用为 **0**；
  WAF 用子串匹配（`security.rs:491-504`）。
- **失控 `tokio::spawn`**：干净。网关只有两个进程级循环，均在 shutdown 时 abort。
- **出站调用缺超时**：除 P0-3 外全部有界（框架 `middleware.rs:232-251`、注册中心均带超时）。
- **框架 metrics 基数**：已保护（4096 / 128 上限 + 原子丢弃计数）。
- **框架幂等存储**：已保护（10,000 条 + 256 MiB）。
- **请求体在层间被克隆**：干净。body 全程是 move，`Request::from_parts` 复用同一缓冲区。
- **CORS 刷新**：读路径干净（一次 `RwLock` 读 + `Arc` 克隆，无 override 时不克隆）；
  仅刷新周期内把同一个 JSON 解析了两次（`cors_dynamic.rs:374` 与 `:403-404`），非热路径。
- **网关路由注册表**（`registry/src/lib.rs:132-146`）：**已按首段建索引**，不是线性扫描。
  注意它与框架的 `HttpRouteManifest` 是两套独立实现——被线性扫描的是框架那套。

---

## 四、建议的落地顺序

1. **立即（低风险高收益）**：P1-4 `is_comments_path` 去分配（改 1 个函数）；
   P0-3 客户端单例 + 超时（改 1 个模块）；
   P1-6 第 1、2 条（缓存 path 哈希、bool 位掩码）。
2. **本迭代**：P0-1 第 3 条（同请求只解析一次路由）→ 再上第 1、2 条（构造期索引）。
   这是唯一能随路由数量线性恶化的项，必须在本迭代根治。
3. **下一迭代**：P0-2（body 双解析：先做声明长度前置拒绝，再评估流式扫描）；
   P1-5 限流存储加驱逐；P1-7 metrics 照搬框架上限。
4. **清理**：P2 四项批量处理。

**验证方式**：建议为 `HttpRouteManifest::match_route` 与 `is_comments_path`
补 criterion 基准（当前**两个仓库都没有任何 benchmark**，这正是本次结论此前只能靠代码形状推断的原因）。

---

## 五、本次审查的边界

- 所有延迟数字来自**隔离微基准**，复现的是审计代码的**精确形状**（同一算法、同样的分配），
  不是端到端真实请求的测量；绝对值随机器变化，**倍数关系**是结论依据。
- 路由清单的**精确总数**未逐一定数：生产按 `platform-foundation-suite-container`
  （56 个 surface）构建，抽样单 workspace 为 2~67 条，量级为数百至上千。
  若需定数，应为组合后的 `HttpRouteManifest` 加一条启动期日志/断言。
- P1-6 中"凭据形状相同即共享限流桶"属于**契约问题**，代码注释暗示有意为之，
  本文仅标记，不作定性。
- 未做运行时压测；若要确认 P0-2 的实际内存峰值，需带真实 body 分布做 profile。

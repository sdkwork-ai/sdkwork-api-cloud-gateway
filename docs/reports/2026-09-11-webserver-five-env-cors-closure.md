# sdkwork-webserver 五环境 0.1.6 上线与 CORS 403 收口验证报告

- 日期：2026-09-11
- 范围：`sdkwork-api-cloud-gateway`（0.1.11）、`sdkwork-webserver`（0.1.6）、`sdkwork-web-framework`、`sdkwork-iam`
- 通道：`bin/` 唯一入口（`bin/docker-image.sh`、`bin/docker-deploy.sh`），WSL Ubuntu 22.04 + 原生 PostgreSQL 18 / Redis 8.6.1
- 结论：**五环境上线成功，端到端验证全绿（`a=0 a2=0 b=0 c=0 d=0 e=0`）**

---

## 1. 报障与目标

用户提交的 403：

```json
{
  "title": "Permission required",
  "status": 403,
  "detail": "CORS origin is not allowed by API policy",
  "instance": "POST /app/v3/api/auth/sessions",
  "code": 40301,
  "operationId": "sessions.create",
  "authProfile": "credential-entry-bootstrap",
  "failedStage": "cors",
  "reason": "forbidden"
}
```

来源页面 `http://im-dev.sdkwork.com/auth/login?redirect=%2F`。

部署目标：

1. WSL 内经 `bin/` 打包并发布镜像，支持五种环境；
2. `sdkwork-webserver` 采用 **standalone** 模式，import 采用 **cloud** 方式；
3. import 的 nginx 配置按 `sdkwork-specs` 规范启动，server 配置可正常对外访问；
4. **每个 webserver 实例 import 全部五个环境**的静态文件 server；
5. 启动成功后输出访问 URL。

---

## 2. 交付产物

| 产物 | 标识 |
| --- | --- |
| 网关镜像 | `registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:0.1.11`（imageId `sha256:61491bf6a1bb…`，364.4 MB） |
| 网关 bundle | `dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.11.bundle` |
| webserver 镜像 | `registry.sdkwork.com/apps/sdkwork-webserver-standalone:0.1.6`（imageId `sha256:886c642f0af4…`） |
| webserver release | `dist/release/sdkwork-webserver-linux-x64-standalone-server-0.1.6.tar.gz`（72366641 B） |
| webserver install bundle | `dist/docker-install/sdkwork-webserver-install-0.1.6.bundle`（本次**重建**，携带修复后的 compose） |

---

## 3. 三个根因（按发现顺序）

### 3.1 根因一：IAM 流水线策略未投影 §5 控制台主机模式

`sdkwork-iam-web-adapter` 的 `iam_web_security_policy()` 只读 `SDKWORK_CORS_ALLOWED_ORIGINS`，未附加注册控制台主机模式，
导致 tower-http 边缘层放行而框架流水线 `Cors` 拦截器拒绝，抛出 `failedStage: cors` 的 40301。
按 `CORS_SPEC` §5/§6（`Embedded host`）改为经 `sdkwork-web-bootstrap` 共享解析器附加模式，半配置 fail-closed。

同步修正 `RegisteredConsoleHosts::matches()` 放宽任意端口的问题（与 §3 及 JS 权威 `tools/cors/registry.mjs#isRegisteredHostOrigin` 冲突）：
带端口的控制台源站一律拒绝。

### 3.2 根因二：`dist/docker-install/*.bundle` 是构建产物，`upgrade` 静默复用旧版

`bin/docker-deploy.sh` 经 `sdkwork_module_install_bundle_dir()` → `sdkwork_newest_install_bundle(dist/docker-install)`
取**目录里最新的 bundle**，而非「请求 tag 对应的 bundle」。仓库中不存在 0.1.6 的 install bundle 时，它**静默复用 0.1.5**，
而 bundle 的 `compose/*.yml` 是整文件覆盖（`env/` 只含 `*.env.example`，不动部署态 env），
于是把源码中已修好的 `docker-compose.bundle-gateway.yml` **盖回旧版**。

症状：`sdkwork-webserver-test-gateway-gateway-1` → `Restarting (1)`，日志

```
Error: failed to build embedded router for `sdkwork-cloudrouter-app-api`:
SDKWORK_CLOUDROUTER_API_KEY_PEPPER still uses published default secret material;
SDKWORK_CLOUDROUTER_TRUSTED_SUBJECT_SECRET ...; SDKWORK_CLOUDROUTER_APP_SESSION_SECRET ...
```

判据：`grep -c 'cloudrouter-local-dev' <bundle>/compose/docker-compose.bundle-gateway.yml` 应为 `0`（旧 bundle 为 4）。

**处置**：`node scripts/docker/package-install-bundle.mjs --version 0.1.6 --skip-image-build`
（该脚本会把 `deployments/docker/docker-compose.bundle-gateway.yml` 拷入 `bundle/compose/`，见脚本 L266-268），
重建后 bundle 内 `cloudrouter-local-dev` 计数归零、六键全部为 `'${…:?Required. Mirror …}'`。

### 3.3 根因三：webserver 自家 env 缺 `SDKWORK_CORS_CONSOLE_HOST_*`，0.1.6 启动即 panic

0.1.6 起 webserver 的 `serve-management` 进程同样校验该 fail-closed 三键
（`crates/sdkwork-webserver-http-host/src/lib.rs:188`）。触发链：

1. 部署态 `bundle/env/<env>.env` **未声明**三键；
2. compose 的 `SDKWORK_CORS_CONSOLE_HOST_{LABELS,SUFFIX,SCHEMES}: ${…:-}` 仍**注入空串**；
3. `registered_console_hosts_from_lookup` 中 `suffix = lookup(SUFFIX)` 得到 `Some("")` ——
   **present-but-empty 也算 configured**（因为生产的空 suffix 有意义）；
4. `labels` 为空 ⇒ 返回 `Err` ⇒ panic：

```
panicked at crates/sdkwork-webserver-http-host/src/lib.rs:188:33:
Web Framework security configuration is invalid: invalid registered console host configuration:
SDKWORK_CORS_CONSOLE_HOST_LABELS must be set when SDKWORK_CORS_CONSOLE_HOST_SUFFIX
or SDKWORK_CORS_CONSOLE_HOST_SCHEMES is configured
```

后果：management 进程死在 3800，healthcheck `curl 127.0.0.1:3800/healthz` 永远失败 → `unhealthy`；
**但数据面 80/443 与 operations 3901 正常**，极易误判为「服务已起」。

**关键澄清：这是部署态 env 漂移，不是镜像缺陷。**
源码 `deployments/docker/env/<env>.env` 早已声明三键并通过门禁
（`node sdkwork-specs/tools/check-cors-standard.mjs --root .` → `1 modules, 19 carriers, 0 carriers with issues`）；
部署态 `bundle/env/<env>.env` 缺失。

**处置**：用 `sdkwork-specs/tools/cors/{env-file,registry,scan}.mjs` 幂等写入五份部署态 env，
权威期望值取自 `scan.mjs#expectedConsolePattern(env)`：
`suffix = registeredEnvironmentSuffix(env)`，`schemes = production ? 'https' : 'http,https'`。
写入前归档到 `/opt/deploy/sdkwork-webserver/archives/20260911T110440Z-console-host-trio/`。

---

## 4. 端到端验证结果

脚本：`.workbuddy/tmp/verify-final.sh`，日志：`.workbuddy/tmp/verify-deployment.log`。
汇总：**`=== VERIFY_SUMMARY a=0 a2=0 b=0 c=0 d=0 e=0 ===`**

### S0 清单

- 网关舰队 5/5 `sdkwork-api-cloud-gateway:0.1.11` `healthy`
- 公网平面侧栈 5/5 `sdkwork-api-cloud-gateway:local` `healthy`
- webserver 5/5 `sdkwork-webserver-standalone:0.1.6` `healthy` `restarts=0`

### A 控制台源站放行（公网边缘，HTTP 环境）— PASS

报障路径 `POST /app/v3/api/auth/sessions`：

| 环境 | 预检 | `Access-Control-Allow-Origin` | POST | 结果 |
| --- | --- | --- | --- | --- |
| development | 204 | `http://im-dev.sdkwork.com` | 401 | `40101` `failedStage: request-context-resolution` |
| test | 204 | `http://im-test.sdkwork.com` | 401 | 同上 |
| staging | 204 | `http://im-staging.sdkwork.com` | 401 | 同上 |
| demo | 204 | `http://im-demo.sdkwork.com` | 401 | 同上 |

即：**CORS 阶段通过**，请求已推进到认证上下文解析。修复前为 `403` + `code 40301` + `failedStage: cors`。

### A2 生产姿态 — PASS

- `http://im.sdkwork.com` 经 `http :18098` → **无 ACAO**（生产 `SCHEMES=https`、空 suffix，符合规范）
- `https :38430` → `000`，**SKIP**：TLS 监听器无对应主机证书（环境前置条件，见 §6）

### B 负向控制 7/7 拒绝 — PASS

未注册基础域 / 带端口控制台源站（§3）/ 后缀注入 `im-dev-test` / 错误环境后缀 `im-test` on dev /
生产 http 控制台源站 / 未注册标签 `imdev` / 未注册标签加后缀 —— 全部拒绝，**未退化为任意子域通配**。

> 注：`SCHEMES=http,https` 时 `https://im-dev.sdkwork.com` **本就合法**，不能作为负例（首版脚本误判，已修正）。

### C 通用 import 平面（逐实例）— PASS

五个实例一致：

```
profile:      cloud
commissioned: development test staging demo production
aggregators:  import.conf  import.conf.cloud  import.conf.standalone
includes.cloud.development = 71
includes.cloud.test        = 71
includes.cloud.staging     = 71
includes.cloud.demo        = 71
includes.cloud.production  = 71
total includes             = 356
### ALL-FIVE-ENVS-IMPORTED[<env>] (cloud profile)
```

数据面启动日志：`serving merged module-imports data plane listeners=2 virtual_hosts=1122 resources=4485 upstreams=75 certificates=1120`。

### D 可达性（公网边缘，HTTP）— PASS 20/20

`im-*` / `account-*` / `server-*` / `api-*/healthz`，dev 80、test 18898、staging 18099、demo 19098、production 18098 全部 `200`。

### E 双平面控制台主机一致性 — PASS

| 环境 | labels | suffix |
| --- | --- | --- |
| development | 111 | `-dev` |
| test | 111 | `-test` |
| staging | 111 | `-staging` |
| demo | 111 | `-demo` |
| production | 111 | `（空）` |

三方一致：**webserver == 侧栈 == 网关舰队**，闭合 `CORS_SPEC` §6 `Embedded host` 投影要求。

---

## 5. 访问 URL

公网边缘端口：dev `80`、test `18898`、staging `18099`、demo `19098`、production `18098`（HTTP）；
HTTPS 另见 §6。

| 环境 | 控制台/应用 | 平台 API | 健康检查 |
| --- | --- | --- | --- |
| development | http://im-dev.sdkwork.com | http://api-dev.sdkwork.com | http://api-dev.sdkwork.com/healthz |
| test | http://im-test.sdkwork.com | http://api-test.sdkwork.com | http://api-test.sdkwork.com/healthz |
| staging | http://im-staging.sdkwork.com | http://api-staging.sdkwork.com | http://api-staging.sdkwork.com/healthz |
| demo | http://im-demo.sdkwork.com | http://api-demo.sdkwork.com | http://api-demo.sdkwork.com/healthz |
| production | http://im.sdkwork.com | http://api.sdkwork.com | http://api.sdkwork.com/healthz |

同环境其他角色标签同样可用（共 111 个控制台标签），例如 `account-*`、`server-*`、`server-admin-*`。

本机（WSL）直连需带 `Host` 头并指定端口，例如：

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:80/ -H 'Host: im-dev.sdkwork.com'
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:18898/healthz -H 'Host: api-test.sdkwork.com'
```

---

## 6. 环境前置条件（非本次回归）

**五环境 HTTPS 均返回 `000` / `tlsv1 alert access denied`**，容器内 `/var/lib/sdkwork/webserver/tls-materials/` 为空
（目录 mtime 为 Aug 30 / Sep 4，早于本次变更）。即 ACME 证书从未签发，TLS 层对任意 SNI 拒绝握手。
生产 HTTP `:18098` 正常（§4 D 段）。HTTPS 恢复需完成证书签发（DNS/ACME 前置）。

---

## 7. 遗留项

1. ⚠️ `docker-compose.bundle.yml` 与 `docker-compose.bundle-gateway.yml` 用 `${VAR:-}` 注入 fail-closed 三键，
   把「未声明」变成「声明为空」，新机从 `*.env.example` 派生时会 panic。
   建议改为 `env_file:` 注入，或让键缺省时不注入（本次以 env 侧补齐收口，未改注入模型以控制上线风险）。
2. bundle 与源码 compose 之间无门禁；建议升级前自动重建 bundle，或校验 compose sha256。
3. 侧栈 compose 的 `*.env.example` 尚未同步 `SDKWORK_CLOUDROUTER_*` 六键（新机派生时 `:?` 会报错 —— 预期 fail-closed）。
4. 跨仓改动尚未提交 git：`sdkwork-iam`、`sdkwork-web-framework`、`sdkwork-webserver`、`sdkwork-api-cloud-gateway`。
5. prod cloudrouter 六件套仍为 dev 占位密钥（既有待决策项）。

---

## 8. 复现命令

```bash
# 网关
bash bin/docker-image.sh build --image-tag 0.1.11
node scripts/docker/package-install-bundle.mjs --skip-image-build
bash bin/docker-deploy.sh upgrade --environment <env> --image-tag 0.1.11 --yes

# webserver（先重建 install bundle，否则 upgrade 会复用旧 compose）
node scripts/webserver-release.mjs package --deployment-profile standalone --version 0.1.6
bash bin/docker-image.sh build --image-tag 0.1.6
node scripts/docker/package-install-bundle.mjs --version 0.1.6 --skip-image-build
bash bin/docker-deploy.sh upgrade --environment <env> --host wsl --deps external --image-tag 0.1.6 --yes

# 验证
bash .workbuddy/tmp/verify-final.sh
```

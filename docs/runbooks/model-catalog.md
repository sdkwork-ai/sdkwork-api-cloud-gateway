# 模型目录 Runbook（sdkwork-api-cloud-gateway）

适用范围：模型厂商 / 资源管理 / 模型映射 控制台页面空白、`POST /backend/v3/api/ai/models/sync` 报
`404 route-not-in-manifest`、或「初始化同步」点了没数据。

原则：**先判定层，再动手。** 这四条症状分属四个互不相干的层，改错层会白改一轮。

## 0. 一条命令判定

```bash
node scripts/verify-model-catalog-readiness.mjs --base-url http://127.0.0.1:3910
```

带凭据才能同时校验数据是否已灌入：

```bash
node scripts/verify-model-catalog-readiness.mjs --base-url http://127.0.0.1:3910 --api-key "$SDKWORK_ADMIN_API_KEY"
```

脚本输出的 `verdict` 就是层归属：

| verdict | 含义 | 修哪层 |
|---|---|---|
| `MISSING` | 404 + `code: 40401` / `route-not-in-manifest` | 路由层：二进制过旧，重建重部署 |
| `EMPTY` | 200 且 `data.items` 为空 | 数据层：catalog 资产缺失或 seed 未执行 |
| `ENVELOPE` | 200 但不是 `{code:0,data,traceId}` | 契约层：owner route crate 绕过了平台 envelope |
| `HANDLER_ERROR` | 5xx | 数据层：库不可连 / 表缺失 |
| `REGISTERED` | 401/403 | 正常：路由已注册，只是没给凭据 |

## 1. 路由层：`MISSING`

404 与 40401 说明请求在 `request-context-resolution` 就被拦下，**鉴权还没开始**，
所以「有没有权限」与这条无关。

```bash
node scripts/check-gateway-api-closure.mjs --root .
```

```bash
pnpm run gateway:validate:container
```

闭包通过（`gateway API closure passed`）说明**构建产物**已声明路由；此时线上仍 404
只可能是运行的是旧二进制。

```bash
pnpm run build:container
```

```bash
bin/docker-deploy.sh install --environment demo
```

## 2. 数据层：`EMPTY`

`ai_model_vendor` 为空有两个成因，按顺序排除。

### 2.1 镜像里有没有 catalog 资产

```bash
node scripts/check-model-catalog-assets.mjs
```

期望 `[model-catalog-assets] PASS`。它同时校验 4 处打包根的
`sdkwork-models.json` + `models/index.json` + `models/meters.json` + `models/protocols.json`，
缺任意一个都会在容器里静默退化成空目录。

### 2.2 容器里跑到哪个 root

```bash
docker exec -it <gateway-container> sh -lc 'echo $SDKWORK_MODELS_CATALOG_ROOT; ls -l $SDKWORK_MODELS_CATALOG_ROOT'
```

期望指向 `/opt/sdkwork/api-gateway/data/sdkwork-models` 且能看到 `sdkwork-models.json` 与 `models/`。

### 2.3 表在不在、数据在不在

```bash
docker exec -it <postgres-container> psql -U "$SDKWORK_DATABASE_USERNAME" -d "$SDKWORK_DATABASE_NAME" -c "SELECT COUNT(*) FROM ai_model_vendor WHERE deleted_at IS NULL;"
```

- 表不存在 ⇒ schema 生命周期没跑。容器走 entrypoint 的 `--migrate-databases`，
  源码/裸机走：

```bash
pnpm run db:init
```

- 表在但为 0 ⇒ 启动时 seed 未触发。看日志：

```bash
docker logs <gateway-container> 2>&1 | grep 'sdkwork.models.catalog_seed'
```

日志会明确给出跳过原因：`ai_model_vendor is not queryable`（表缺失）、
`official catalog bundle is unavailable`（资产缺失）、
`official bundle declares no vendors`（挂到了空目录）。

### 2.4 手动补灌

```bash
node scripts/verify-model-catalog-readiness.mjs --base-url http://127.0.0.1:3910 --api-key "$SDKWORK_ADMIN_API_KEY" --allow-mutations
```

`--allow-mutations` 会真的打一次 `POST /ai/models/sync`。该接口是显式刷新路径，
与首启 seed 幂等并存，可安全重试。

## 3. 契约层：`ENVELOPE`

```bash
node ../sdkwork-specs/tools/check-api-response-envelope.mjs --root ../sdkwork-models
```

```bash
node tools/check-admin-sdk-route-parity.mjs
```

第二条在 `sdkwork-models` 仓执行，校验 SDK operations ↔ ROUTES 描述符 ↔ http route manifest
三方一致（backend 30/30/30、app 10/10/10）。不一致说明前端调的路径后端没实现，或反之。

## 4. 并发首启注意

多副本同时首启会各自看到空表。`catalog_seed` 用事务级 advisory lock
（`SDKWORKMODELS` 常量键）串行化，第二次进来会重查计数并直接返回
`AlreadySeeded`，不会互相软删数据。因此：

- 不要因为「多副本打了一条 seed 日志」就判定重复导入；
- 只有一条 `model catalog seeded` 是预期行为，其余是 `debug` 级跳过。

## 5. 验收清单

四条全绿才算可交付：

```bash
node scripts/check-model-catalog-assets.mjs
```

```bash
node scripts/check-gateway-api-closure.mjs --root .
```

```bash
node scripts/verify-model-catalog-readiness.mjs --base-url http://127.0.0.1:3910
```

```bash
cd ../sdkwork-models && pnpm run _sdkwork:check
```

# Model Catalog Runbook (sdkwork-api-cloud-gateway)

Covers: blank model-vendor / resource / model-mapping console pages,
`POST /backend/v3/api/ai/models/sync` answering `404 route-not-in-manifest`,
and a "sync" button that produces no data.

Rule: **identify the layer first, then act.** Those symptoms belong to four
independent layers; fixing the wrong one wastes a full cycle.

## 0. One command to classify

```bash
node scripts/verify-model-catalog-readiness.mjs --base-url http://127.0.0.1:3910
```

Add credentials to also assert that the catalog is actually seeded:

```bash
node scripts/verify-model-catalog-readiness.mjs --base-url http://127.0.0.1:3910 --api-key "$SDKWORK_ADMIN_API_KEY"
```

The reported `verdict` is the layer:

| verdict | Meaning | Layer to fix |
|---|---|---|
| `MISSING` | 404 + `code: 40401` / `route-not-in-manifest` | Routing: stale binary, rebuild and redeploy |
| `EMPTY` | 200 but `data.items` is empty | Data: catalog assets missing or seed never ran |
| `ENVELOPE` | 200 but not `{code:0,data,traceId}` | Contract: owner route crate bypassed the platform envelope |
| `HANDLER_ERROR` | 5xx | Data: database unreachable or tables missing |
| `REGISTERED` | 401/403 | Healthy: route registered, credentials not supplied |

## 1. Routing layer: `MISSING`

A 404 with 40401 means the request was rejected at
`request-context-resolution`, **before authentication**. Permissions are
irrelevant here.

```bash
node scripts/check-gateway-api-closure.mjs --root .
```

```bash
pnpm run gateway:validate:container
```

A passing closure (`gateway API closure passed`) proves the *build* declares the
routes. A live 404 then only means an old binary is running.

```bash
pnpm run build:container
```

```bash
bin/docker-deploy.sh install --environment demo
```

## 2. Data layer: `EMPTY`

An empty `ai_model_vendor` has two causes; rule them out in order.

### 2.1 Does the image carry the catalog assets

```bash
node scripts/check-model-catalog-assets.mjs
```

Expect `[model-catalog-assets] PASS`. It validates `sdkwork-models.json` +
`models/index.json` + `models/meters.json` + `models/protocols.json` across all
four packaged roots; any missing file silently degrades to an empty catalog
inside the container.

### 2.2 Which root the container resolves

```bash
docker exec -it <gateway-container> sh -lc 'echo $SDKWORK_MODELS_CATALOG_ROOT; ls -l $SDKWORK_MODELS_CATALOG_ROOT'
```

Expect `/opt/sdkwork/api-gateway/data/sdkwork-models` containing
`sdkwork-models.json` and `models/`.

### 2.3 Do the tables and rows exist

```bash
docker exec -it <postgres-container> psql -U "$SDKWORK_DATABASE_USERNAME" -d "$SDKWORK_DATABASE_NAME" -c "SELECT COUNT(*) FROM ai_model_vendor WHERE deleted_at IS NULL;"
```

- Table missing => the schema lifecycle did not run. Containers get it from the
  entrypoint `--migrate-databases`; source checkouts run:

```bash
pnpm run db:init
```

- Table present but zero rows => the boot seed did not fire. Read the log:

```bash
docker logs <gateway-container> 2>&1 | grep 'sdkwork.models.catalog_seed'
```

The log states the skip reason: `ai_model_vendor is not queryable` (no table),
`official catalog bundle is unavailable` (no assets),
`official bundle declares no vendors` (empty root mounted).

### 2.4 Seed manually

```bash
node scripts/verify-model-catalog-readiness.mjs --base-url http://127.0.0.1:3910 --api-key "$SDKWORK_ADMIN_API_KEY" --allow-mutations
```

`--allow-mutations` performs one real `POST /ai/models/sync`. That endpoint is
the explicit refresh path and is idempotent against the boot seed, so retrying
is safe.

## 3. Contract layer: `ENVELOPE`

```bash
node ../sdkwork-specs/tools/check-api-response-envelope.mjs --root ../sdkwork-models
```

```bash
node tools/check-admin-sdk-route-parity.mjs
```

The second command runs in `sdkwork-models` and cross-checks SDK operations
against ROUTES descriptors and the http route manifest (backend 30/30/30,
app 10/10/10). A mismatch means the frontend calls a path the backend does not
serve, or the reverse.

## 4. Concurrent first boot

Replicas booting together would each observe an empty table. `catalog_seed`
serializes on a transaction-scoped advisory lock (constant key derived from
`SDKWORKMODELS`); the loser re-checks the row count and returns
`AlreadySeeded` without replaying upserts or soft-deleting rows. Therefore:

- do not read "only one seed log line across N replicas" as a partial seed;
- one `model catalog seeded` line is expected, the rest are `debug` skips.

## 5. Acceptance checklist

All four green:

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

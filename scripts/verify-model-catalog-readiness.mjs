#!/usr/bin/env node
/**
 * Live readiness probe for the Cloud Router model catalog admin API.
 *
 * Static gates (`check-model-catalog-assets.mjs`, OpenAPI/SDK route parity,
 * `check-gateway-api-closure`) prove that a *build* declares the model vendor,
 * resource group, and model mapping routes. They cannot prove that a running
 * deployment actually serves them, because a gateway whose composed route
 * manifest omits a dependency contribution answers HTTP 404 with
 * `code: 40401` / `reason: "route-not-in-manifest"` at
 * `request-context-resolution` — before authentication is even evaluated.
 *
 * That distinction is what makes this probe unambiguous:
 *   - 404 + 40401            => the route is NOT in the gateway manifest (fail)
 *   - 401 / 403              => the route IS registered and requires credentials
 *   - 2xx                    => the route is registered and reachable
 *
 * A green run therefore proves three things at once: the gateway manifest
 * carries the models-catalog contribution, the deployment ships the catalog
 * bundle, and the first-install catalog seed populated `ai_model_vendor`
 * (an empty vendor list still fails, because that is the exact regression this
 * probe exists to catch).
 *
 * Usage:
 *   node scripts/verify-model-catalog-readiness.mjs --base-url http://127.0.0.1:3900
 *   node scripts/verify-model-catalog-readiness.mjs --base-url https://api.example.com \
 *     --api-key "$SDKWORK_ADMIN_API_KEY"
 *   node scripts/verify-model-catalog-readiness.mjs --base-url http://127.0.0.1:3900 \
 *     --bearer-token "$TOKEN" --access-token "$ACCESS" --allow-mutations
 *
 * Exit codes: 0 ready, 1 not ready, 2 invalid invocation.
 */

import process from 'node:process';

const READ_ROUTES = [
  { method: 'GET', path: '/backend/v3/api/ai/model_vendors', expectItems: true },
  { method: 'GET', path: '/backend/v3/api/ai/models', expectItems: true },
  { method: 'GET', path: '/backend/v3/api/ai/resource_groups', expectItems: true },
  { method: 'GET', path: '/backend/v3/api/ai/resources', expectItems: true },
  { method: 'GET', path: '/backend/v3/api/ai/model_mappings', expectItems: true },
  { method: 'GET', path: '/backend/v3/api/ai/model_rankings', expectItems: true },
  { method: 'GET', path: '/backend/v3/api/ai/model_rankings/status', expectItems: false },
  { method: 'GET', path: '/backend/v3/api/ai/model_rankings/jobs', expectItems: true },
  { method: 'GET', path: '/backend/v3/api/ai/voices', expectItems: true },
  { method: 'GET', path: '/backend/v3/api/ai/video_profiles', expectItems: true },
];

const MUTATION_ROUTES = [
  { method: 'POST', path: '/backend/v3/api/ai/models/sync', expectItems: false },
];

function parseArgs(argv) {
  const args = {
    baseUrl: '',
    apiKey: process.env.SDKWORK_ADMIN_API_KEY ?? '',
    bearerToken: process.env.SDKWORK_ADMIN_BEARER_TOKEN ?? '',
    accessToken: process.env.SDKWORK_ADMIN_ACCESS_TOKEN ?? '',
    allowMutations: false,
    timeoutMs: 15000,
  };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--base-url') args.baseUrl = argv[++i] ?? '';
    else if (arg === '--api-key') args.apiKey = argv[++i] ?? '';
    else if (arg === '--bearer-token') args.bearerToken = argv[++i] ?? '';
    else if (arg === '--access-token') args.accessToken = argv[++i] ?? '';
    else if (arg === '--allow-mutations') args.allowMutations = true;
    else if (arg === '--timeout-ms') args.timeoutMs = Number(argv[++i] ?? '') || 15000;
    else if (arg === '--help' || arg === '-h') args.help = true;
    else if (arg.startsWith('--')) {
      console.error(`unknown option: ${arg}`);
      process.exit(2);
    }
  }
  return args;
}

function usage() {
  console.log(`Usage: node scripts/verify-model-catalog-readiness.mjs --base-url <url> [options]

Options:
  --base-url <url>        Gateway origin to probe (required), e.g. http://127.0.0.1:3900
  --api-key <key>         Value for the X-API-Key credential header (env: SDKWORK_ADMIN_API_KEY)
  --bearer-token <token>  Value for the Authorization: Bearer header (env: SDKWORK_ADMIN_BEARER_TOKEN)
  --access-token <token>  Value for the Access-Token header (env: SDKWORK_ADMIN_ACCESS_TOKEN)
  --allow-mutations       Also POST /backend/v3/api/ai/models/sync (refresh path)
  --timeout-ms <ms>       Per-request timeout (default 15000)

Without credentials the probe still proves route registration: a registered
route answers 401/403, while a route missing from the gateway manifest answers
404 with code 40401.`);
}

function buildHeaders(args) {
  const headers = { accept: 'application/json' };
  if (args.apiKey) headers['x-api-key'] = args.apiKey;
  if (args.bearerToken) headers.authorization = `Bearer ${args.bearerToken}`;
  if (args.accessToken) headers['access-token'] = args.accessToken;
  if (args.bearerToken || args.accessToken) headers['content-type'] = 'application/json';
  return headers;
}

/** Route-not-in-manifest is the single failure class this probe must detect. */
function isRouteNotInManifest(status, body) {
  if (status !== 404) return false;
  if (!body || typeof body !== 'object') return false;
  return (
    body.code === 40401
    || body.reason === 'route-not-in-manifest'
    || body.failedStage === 'request-context-resolution'
    || (typeof body.detail === 'string' && body.detail.includes('not registered in the gateway route manifest'))
  );
}

function countItems(body) {
  const data = body?.data;
  if (Array.isArray(data)) return data.length;
  if (!data || typeof data !== 'object') return null;
  for (const key of ['items', 'list', 'records', 'rows', 'results']) {
    if (Array.isArray(data[key])) return data[key].length;
  }
  return null;
}

function hasEnvelope(body) {
  return Boolean(body) && typeof body === 'object' && body.code === 0 && 'data' in body;
}

async function probe(args, route) {
  const url = `${args.baseUrl.replace(/\/+$/u, '')}${route.path}`;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), args.timeoutMs);
  try {
    const response = await fetch(url, {
      method: route.method,
      headers: buildHeaders(args),
      body: route.method === 'GET' || route.method === 'DELETE' ? undefined : '{}',
      signal: controller.signal,
    });
    const text = await response.text();
    let body = null;
    if (text) {
      try {
        body = JSON.parse(text);
      } catch {
        body = null;
      }
    }
    return { status: response.status, body, text };
  } finally {
    clearTimeout(timer);
  }
}

function verdictFor(route, result) {
  const { status, body } = result;
  if (isRouteNotInManifest(status, body)) {
    return { verdict: 'MISSING', detail: 'route is not registered in the gateway route manifest (40401)' };
  }
  if (status === 401 || status === 403) {
    return { verdict: 'REGISTERED', detail: 'registered; credentials required (no credential supplied or rejected)' };
  }
  if (status >= 200 && status < 300) {
    if (!hasEnvelope(body)) {
      return { verdict: 'ENVELOPE', detail: 'reachable but response is missing the {code:0,data,traceId} envelope' };
    }
    const items = countItems(body);
    if (route.expectItems && items === 0) {
      return { verdict: 'EMPTY', detail: 'reachable but data.items is empty; the catalog bundle is missing or the seed did not run' };
    }
    return {
      verdict: 'OK',
      detail: items === null ? 'reachable, envelope ok' : `reachable, envelope ok, data.items=${items}`,
    };
  }
  if (status === 404) {
    return { verdict: 'NOT_FOUND', detail: '404 without the route-not-in-manifest marker (resource lookup, not a manifest gap)' };
  }
  if (status >= 500) {
    return { verdict: 'HANDLER_ERROR', detail: `handler failed with HTTP ${status}` };
  }
  return { verdict: 'REJECTED', detail: `registered; handler rejected the request with HTTP ${status}` };
}

function pad(value, width) {
  const text = String(value);
  return text.length >= width ? text : text + ' '.repeat(width - text.length);
}

async function main() {
  const args = parseArgs(process.argv);
  if (args.help) {
    usage();
    process.exit(0);
  }
  if (!args.baseUrl) {
    console.error('[verify-model-catalog] --base-url is required');
    usage();
    process.exit(2);
  }

  const routes = args.allowMutations ? [...READ_ROUTES, ...MUTATION_ROUTES] : READ_ROUTES;
  const failures = [];
  const rows = [];

  console.log(`[verify-model-catalog] gateway=${args.baseUrl}`);
  console.log(
    `[verify-model-catalog] credential=${args.apiKey ? 'x-api-key' : args.bearerToken || args.accessToken ? 'bearer+access-token' : 'none (registration-only probe)'}`,
  );

  for (const route of routes) {
    let result;
    try {
      result = await probe(args, route);
    } catch (error) {
      console.error(`[verify-model-catalog] request failed: ${route.method} ${route.path}: ${error.message}`);
      process.exit(2);
    }
    const { verdict, detail } = verdictFor(route, result);
    rows.push({ ...route, status: result.status, verdict, detail });
    const hard = verdict === 'MISSING' || verdict === 'ENVELOPE' || verdict === 'EMPTY' || verdict === 'HANDLER_ERROR';
    if (hard) failures.push({ ...route, status: result.status, verdict, detail });
  }

  const methodWidth = Math.max(...rows.map((row) => row.method.length));
  const pathWidth = Math.max(...rows.map((row) => row.path.length));
  for (const row of rows) {
    console.log(`  ${pad(row.method, methodWidth)}  ${pad(row.path, pathWidth)}  ${pad(row.status, 3)}  ${pad(row.verdict, 12)}  ${row.detail}`);
  }

  if (failures.length > 0) {
    console.error(`\n[verify-model-catalog] NOT READY: ${failures.length}/${rows.length} checks failed`);
    for (const failure of failures) {
      console.error(`  - ${failure.method} ${failure.path}: ${failure.verdict} (${failure.detail})`);
    }
    console.error(
      '\nRemediation:\n'
      + '  1. route MISSING  -> the gateway binary predates the composed backend manifest; rebuild and redeploy.\n'
      + '  2. route EMPTY    -> verify SDKWORK_MODELS_CATALOG_ROOT inside the container and that the catalog bundle\n'
      + '                       (sdkwork-models.json + models/) is present; then POST /ai/models/sync once.\n'
      + '  3. route ENVELOPE -> the response bypassed the platform envelope; check the owning route crate.\n',
    );
    process.exit(1);
  }

  console.log(`\n[verify-model-catalog] READY: ${rows.length}/${rows.length} checks passed`);
  if (!args.apiKey && !args.bearerToken && !args.accessToken) {
    console.log(
      '[verify-model-catalog] note: no credentials supplied, so only route registration was proven.\n'
      + '  Re-run with SDKWORK_ADMIN_API_KEY to also assert that the catalog is seeded (data.items > 0).',
    );
  }
}

main().catch((error) => {
  console.error(`[verify-model-catalog] unexpected failure: ${error?.stack ?? error}`);
  process.exit(2);
});

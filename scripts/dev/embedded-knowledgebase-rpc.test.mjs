import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  resolveEmbeddedKnowledgebaseRpcPlan,
  stopEmbeddedKnowledgebaseRpc,
} from './embedded-knowledgebase-rpc.mjs';

const gatewayRoot = path.resolve('.');
let runtimeStateRoot;

test.beforeEach(() => {
  runtimeStateRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'sdkwork-gateway-rpc-test-'));
});

test.afterEach(() => {
  fs.rmSync(runtimeStateRoot, { recursive: true, force: true });
});

test('standalone development resolves a complete managed Knowledgebase RPC pair', () => {
  const plan = resolveEmbeddedKnowledgebaseRpcPlan({
    databaseSchema: 'sdkwork_ai_dev',
    databaseUrl: 'postgres://localhost/sdkwork_ai_dev',
    env: {},
    gatewayRoot,
    pgOptions: '-csearch_path=sdkwork_ai_dev',
    runtimeStateRoot,
  });

  assert.equal(
    plan.clientEnv.SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_ENDPOINT,
    'https://127.0.0.1:50054',
  );
  assert.equal(
    plan.clientEnv.SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_TLS_DOMAIN,
    'knowledgebase.local',
  );
  assert.equal(
    plan.hostEnv.SDKWORK_KNOWLEDGEBASE_RPC_BIND_ADDR,
    '127.0.0.1:50054',
  );
  assert.equal(plan.hostEnv.SDKWORK_KNOWLEDGEBASE_RPC_MTLS_ENABLED, 'true');
  assert.equal(plan.hostEnv.PGOPTIONS, '-csearch_path=sdkwork_ai_dev');
  assert.equal(
    plan.hostEnv.SDKWORK_DATABASE_SCHEMA,
    'sdkwork_ai_dev',
  );
  assert.equal(plan.hostEnv.SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC, 'false');
  assert.equal(
    plan.hostEnv.SDKWORK_KNOWLEDGEBASE_RPC_IM_CALLER_CONTEXT_SIGNING_KEY,
    plan.clientEnv.SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CALLER_CONTEXT_SIGNING_KEY,
  );
  assert.ok(
    plan.clientEnv.SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CERT_PATH.startsWith(
      path.join(runtimeStateRoot, 'knowledgebase-rpc'),
    ),
  );
  const relativeRuntimeRoot = path.relative(gatewayRoot, plan.runtimeRoot);
  assert.ok(path.isAbsolute(relativeRuntimeRoot) || relativeRuntimeRoot.startsWith('..'));
});

test('managed Knowledgebase RPC cleanup removes its disposable runtime state', async () => {
  const plan = resolveEmbeddedKnowledgebaseRpcPlan({
    databaseSchema: 'sdkwork_ai_dev',
    databaseUrl: 'postgres://localhost/sdkwork_ai_dev',
    env: {},
    gatewayRoot,
    pgOptions: '-csearch_path=sdkwork_ai_dev',
    runtimeStateRoot,
  });
  fs.mkdirSync(plan.runtimeRoot, { recursive: true });
  fs.writeFileSync(path.join(plan.runtimeRoot, 'secret.pem'), 'temporary');

  await stopEmbeddedKnowledgebaseRpc(undefined, plan);

  assert.equal(fs.existsSync(plan.runtimeRoot), false);
});

test('managed Knowledgebase RPC rejects source-local runtime state', () => {
  assert.throws(
    () => resolveEmbeddedKnowledgebaseRpcPlan({
      databaseSchema: 'sdkwork_ai_dev',
      databaseUrl: 'postgres://localhost/sdkwork_ai_dev',
      env: {},
      gatewayRoot,
      pgOptions: '-csearch_path=sdkwork_ai_dev',
      runtimeStateRoot: path.join(gatewayRoot, '.runtime'),
    }),
    /outside the source tree/u,
  );
});

test('an explicit Knowledgebase RPC client remains deployment-owned', () => {
  assert.equal(
    resolveEmbeddedKnowledgebaseRpcPlan({
      databaseSchema: 'sdkwork_ai_dev',
      databaseUrl: 'postgres://localhost/sdkwork_ai_dev',
      env: {
        SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_ENDPOINT:
          'https://knowledgebase.internal:7443',
      },
      gatewayRoot,
      pgOptions: '-csearch_path=sdkwork_ai_dev',
      runtimeStateRoot,
    }),
    undefined,
  );
});

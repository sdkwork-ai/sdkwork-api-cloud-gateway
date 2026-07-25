import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';

import { resolveEmbeddedKnowledgebaseRpcPlan } from './embedded-knowledgebase-rpc.mjs';

const gatewayRoot = path.resolve('.');

test('standalone development resolves a complete managed Knowledgebase RPC pair', () => {
  const plan = resolveEmbeddedKnowledgebaseRpcPlan({
    databaseSchema: 'sdkwork_ai_dev',
    databaseUrl: 'postgres://localhost/sdkwork_ai_dev',
    env: {},
    gatewayRoot,
    pgOptions: '-csearch_path=sdkwork_ai_dev',
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
    plan.hostEnv.SDKWORK_KNOWLEDGEBASE_DATABASE_SCHEMA,
    'sdkwork_ai_dev',
  );
  assert.equal(plan.hostEnv.SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC, 'false');
  assert.equal(
    plan.hostEnv.SDKWORK_KNOWLEDGEBASE_RPC_IM_CALLER_CONTEXT_SIGNING_KEY,
    plan.clientEnv.SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CALLER_CONTEXT_SIGNING_KEY,
  );
  assert.ok(
    plan.clientEnv.SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CERT_PATH.startsWith(
      path.join(gatewayRoot, '.runtime'),
    ),
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
    }),
    undefined,
  );
});

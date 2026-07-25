import assert from 'node:assert/strict';
import test from 'node:test';

import {
  resolveEmbeddedDatabaseEnv,
  resolveEmbeddedDependencyEnv,
  resolveEmbeddedPostgresUrl,
  resolveTemporaryDatabaseDriverEnv,
} from './sdkwork-api-cloud-gateway-dev-env.mjs';

test('development database driver exceptions use the declared process defaults', () => {
  assert.deepEqual(resolveTemporaryDatabaseDriverEnv({}), {
    SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION: 'true',
    SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT: '2',
  });
});

test('development database driver exceptions preserve explicit overrides', () => {
  assert.deepEqual(
    resolveTemporaryDatabaseDriverEnv({
      SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION: 'false',
      SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT: '1',
    }),
    {
      SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION: 'false',
      SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT: '1',
    },
  );
});

test('embedded database consumers share the PostgreSQL profile without fallback stores', () => {
  const databaseUrl = 'postgres://localhost/sdkwork_ai_dev';
  const resolved = resolveEmbeddedDatabaseEnv({
    databaseUrl,
    schema: 'sdkwork_ai_dev',
    env: {},
  });

  assert.equal(resolved.SDKWORK_IM_DATABASE_URL, databaseUrl);
  assert.equal(resolved.SDKWORK_IM_DATABASE_SCHEMA, 'sdkwork_ai_dev');
  assert.equal(resolved.SDKWORK_KNOWLEDGEBASE_DATABASE_URL, databaseUrl);
  assert.equal(resolved.SDKWORK_KNOWLEDGEBASE_DATABASE_SCHEMA, 'sdkwork_ai_dev');
  assert.equal(resolved.SDKWORK_AIOT_DEVICE_DATABASE_URL, databaseUrl);
  assert.equal(resolved.SDKWORK_AIOT_DEVICE_DATABASE_TABLE_PREFIX, 'iot_');
  assert.equal(resolved.SDKWORK_RTC_STATE_DATABASE_URL, databaseUrl);
  assert.equal(resolved.SDKWORK_RTC_OUTBOX_DATABASE_URL, databaseUrl);
  assert.equal(resolved.SDKWORK_RTC_STATE_REQUIRE_DURABLE, 'true');
  assert.equal(resolved.SDKWORK_RTC_REQUIRE_OUTBOX, 'true');
});

test('embedded PostgreSQL URL uses cross-driver search_path options syntax', () => {
  const resolved = resolveEmbeddedPostgresUrl(
    'postgres://user:password@localhost/sdkwork_ai_dev?sslmode=disable',
    'sdkwork_ai_dev',
  );
  const url = new URL(resolved);

  assert.equal(url.searchParams.get('options'), '-csearch_path=sdkwork_ai_dev');
  assert.doesNotMatch(resolved, /\+search_path/u);
});

test('embedded PostgreSQL URL replaces legacy spaced search_path options', () => {
  const resolved = resolveEmbeddedPostgresUrl(
    'postgres://user:password@localhost/sdkwork_ai_dev?sslmode=disable&options=-c+search_path%3Dlegacy',
    'sdkwork_ai_dev',
  );
  const url = new URL(resolved);

  assert.equal(url.searchParams.get('options'), '-csearch_path=sdkwork_ai_dev');
  assert.doesNotMatch(resolved, /\+search_path/u);
  assert.doesNotMatch(resolved, /legacy/u);
});

test('embedded dependencies use the local gateway origin by default', () => {
  const resolved = resolveEmbeddedDependencyEnv({});

  assert.equal(resolved.SDKWORK_CLAWROUTER_OPEN_API_BASE_URL, 'http://127.0.0.1:3900');
  assert.equal(
    resolved.SDKWORK_CLAW_API_KEY_PEPPER,
    'sdkwork-clawrouter-local-dev-secret-20260507',
  );
  assert.equal(
    resolved.SDKWORK_CLAW_TRUSTED_SUBJECT_SECRET,
    'sdkwork-clawrouter-local-dev-secret-20260507',
  );
  assert.equal(
    resolved.SDKWORK_CLAW_APP_SESSION_SECRET,
    'sdkwork-clawrouter-local-dev-secret-20260507',
  );
  assert.equal(
    resolved.SDKWORK_CLAW_PAYMENT_WEBHOOK_SECRET,
    'sdkwork-clawrouter-local-dev-secret-20260507',
  );
});

test('embedded IM receives the selected lifecycle environment', () => {
  assert.equal(
    resolveEmbeddedDependencyEnv({
      SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT: 'development',
    }).SDKWORK_IM_ENVIRONMENT,
    'development',
  );
  assert.equal(
    resolveEmbeddedDependencyEnv({
      SDKWORK_ENVIRONMENT: 'development',
      SDKWORK_IM_ENVIRONMENT: 'test',
    }).SDKWORK_IM_ENVIRONMENT,
    'test',
  );
});

test('embedded dependency env preserves explicit ClawRouter overrides', () => {
  const explicit = resolveEmbeddedDependencyEnv({
    SDKWORK_CLAWROUTER_OPEN_API_BASE_URL: ' https://clawrouter.example.com ',
    CLAWROUTER_OPEN_API_BASE_URL: 'https://legacy.example.com',
    SDKWORK_CLAW_API_KEY_PEPPER: 'explicit-api-key-pepper',
    SDKWORK_CLAW_TRUSTED_SUBJECT_SECRET: 'explicit-trusted-subject-secret',
    SDKWORK_CLAW_APP_SESSION_SECRET: 'explicit-app-session-secret',
    SDKWORK_CLAW_PAYMENT_WEBHOOK_SECRET: 'explicit-payment-webhook-secret',
  });

  assert.equal(
    explicit.SDKWORK_CLAWROUTER_OPEN_API_BASE_URL,
    'https://clawrouter.example.com',
  );
  assert.equal(explicit.SDKWORK_CLAW_API_KEY_PEPPER, 'explicit-api-key-pepper');
  assert.equal(
    explicit.SDKWORK_CLAW_TRUSTED_SUBJECT_SECRET,
    'explicit-trusted-subject-secret',
  );
  assert.equal(explicit.SDKWORK_CLAW_APP_SESSION_SECRET, 'explicit-app-session-secret');
  assert.equal(
    explicit.SDKWORK_CLAW_PAYMENT_WEBHOOK_SECRET,
    'explicit-payment-webhook-secret',
  );
  assert.equal(
    resolveEmbeddedDependencyEnv({
      CLAWROUTER_OPEN_API_BASE_URL: 'https://legacy.example.com',
    }).SDKWORK_CLAWROUTER_OPEN_API_BASE_URL,
    'https://legacy.example.com',
  );
});

test('embedded dependency env derives a reachable origin from a custom bind', () => {
  assert.equal(
    resolveEmbeddedDependencyEnv({ SDKWORK_API_CLOUD_GATEWAY_BIND: '0.0.0.0:4900' })
      .SDKWORK_CLAWROUTER_OPEN_API_BASE_URL,
    'http://127.0.0.1:4900',
  );
});

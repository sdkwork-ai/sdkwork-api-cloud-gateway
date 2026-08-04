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

  assert.deepEqual(resolved, {
    SDKWORK_DATABASE_URL: databaseUrl,
    SDKWORK_DATABASE_SCHEMA: 'sdkwork_ai_dev',
    SDKWORK_AIOT_DEVICE_STORAGE_TABLE_PREFIX: 'iot_',
    SDKWORK_IM_ID_NODE_ID: '0',
    SDKWORK_RTC_STATE_REQUIRE_DURABLE: 'true',
    SDKWORK_RTC_REQUIRE_OUTBOX: 'true',
  });
});

test('embedded database env preserves business storage overrides without module database aliases', () => {
  const resolved = resolveEmbeddedDatabaseEnv({
    databaseUrl: 'postgres://localhost/sdkwork_ai_dev',
    schema: 'sdkwork_ai_dev',
    env: {
      SDKWORK_AIOT_DEVICE_STORAGE_TABLE_PREFIX: 'device_',
      SDKWORK_IM_ID_NODE_ID: '7',
    },
  });

  assert.equal(resolved.SDKWORK_AIOT_DEVICE_STORAGE_TABLE_PREFIX, 'device_');
  assert.equal(resolved.SDKWORK_IM_ID_NODE_ID, '7');
  assert.deepEqual(
    Object.keys(resolved).filter((key) => /^SDKWORK_.+_DATABASE_/u.test(key)),
    [],
  );
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

  assert.equal(resolved.SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL, 'http://127.0.0.1:3900');
  assert.equal(
    resolved.SDKWORK_CLOUDROUTER_API_KEY_PEPPER,
    'sdkwork-cloudrouter-local-dev-secret-20260507',
  );
  assert.equal(
    resolved.SDKWORK_CLOUDROUTER_TRUSTED_SUBJECT_SECRET,
    'sdkwork-cloudrouter-local-dev-secret-20260507',
  );
  assert.equal(
    resolved.SDKWORK_CLOUDROUTER_APP_SESSION_SECRET,
    'sdkwork-cloudrouter-local-dev-secret-20260507',
  );
  assert.equal(
    resolved.SDKWORK_CLOUDROUTER_PAYMENT_WEBHOOK_SECRET,
    'sdkwork-cloudrouter-local-dev-secret-20260507',
  );
});

test('embedded modules receive the selected lifecycle environment', () => {
  const resolved = resolveEmbeddedDependencyEnv({
    SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT: 'development',
  });
  assert.equal(resolved.SDKWORK_IM_ENVIRONMENT, 'development');
  assert.equal(resolved.SDKWORK_KNOWLEDGEBASE_ENVIRONMENT, 'development');
});

test('embedded module environment overrides remain independent', () => {
  const resolved = resolveEmbeddedDependencyEnv({
    SDKWORK_ENVIRONMENT: 'development',
    SDKWORK_IM_ENVIRONMENT: 'test',
    SDKWORK_KNOWLEDGEBASE_ENVIRONMENT: 'staging',
  });
  assert.equal(
    resolved.SDKWORK_IM_ENVIRONMENT,
    'test',
  );
  assert.equal(resolved.SDKWORK_KNOWLEDGEBASE_ENVIRONMENT, 'staging');
});

test('embedded dependency env preserves explicit CloudRouter overrides', () => {
  const explicit = resolveEmbeddedDependencyEnv({
    SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL: ' https://cloudrouter.example.com ',
    CLOUDROUTER_OPEN_API_BASE_URL: 'https://legacy.example.com',
    SDKWORK_CLOUDROUTER_API_KEY_PEPPER: 'explicit-api-key-pepper',
    SDKWORK_CLOUDROUTER_TRUSTED_SUBJECT_SECRET: 'explicit-trusted-subject-secret',
    SDKWORK_CLOUDROUTER_APP_SESSION_SECRET: 'explicit-app-session-secret',
    SDKWORK_CLOUDROUTER_PAYMENT_WEBHOOK_SECRET: 'explicit-payment-webhook-secret',
  });

  assert.equal(
    explicit.SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL,
    'https://cloudrouter.example.com',
  );
  assert.equal(explicit.SDKWORK_CLOUDROUTER_API_KEY_PEPPER, 'explicit-api-key-pepper');
  assert.equal(
    explicit.SDKWORK_CLOUDROUTER_TRUSTED_SUBJECT_SECRET,
    'explicit-trusted-subject-secret',
  );
  assert.equal(explicit.SDKWORK_CLOUDROUTER_APP_SESSION_SECRET, 'explicit-app-session-secret');
  assert.equal(
    explicit.SDKWORK_CLOUDROUTER_PAYMENT_WEBHOOK_SECRET,
    'explicit-payment-webhook-secret',
  );
  assert.equal(
    resolveEmbeddedDependencyEnv({
      CLOUDROUTER_OPEN_API_BASE_URL: 'https://legacy.example.com',
    }).SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL,
    'https://legacy.example.com',
  );
});

test('embedded dependency env derives a reachable origin from a custom bind', () => {
  assert.equal(
    resolveEmbeddedDependencyEnv({ SDKWORK_API_CLOUD_GATEWAY_BIND: '0.0.0.0:4900' })
      .SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL,
    'http://127.0.0.1:4900',
  );
});

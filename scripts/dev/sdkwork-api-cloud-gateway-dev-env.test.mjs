// WORKSPACE-PATH:allow-fixture: fixtures name a foreign checkout root, drive, or home directory to exercise path handling, so the literal is the value under assertion rather than a binding this build resolves
import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';

import {
  describeCloudRouterCredentialKeyRingSource,
  resolveCloudRouterCredentialKeyRing,
  resolveEmbeddedCanvasSqliteUrl,
  resolveEmbeddedDatabaseEnv,
  resolveEmbeddedDependencyEnv,
  resolveEmbeddedMemoryObjectStoreRoot,
  resolveEmbeddedPostgresUrl,
  resolveTemporaryDatabaseDriverEnv,
} from './sdkwork-api-cloud-gateway-dev-env.mjs';

test('embedded canvas SQLite store lives in the runtime state directory', () => {
  const resolved = resolveEmbeddedCanvasSqliteUrl({
    runtimeStateDirectory: 'C:\\Users\\dev\\AppData\\Local\\sdkwork\\state',
  });
  assert.equal(
    resolved,
    'sqlite:C:/Users/dev/AppData/Local/sdkwork/state/canvas-pages.db?mode=rwc',
  );
});

test('embedded canvas SQLite URL preserves explicit overrides', () => {
  const resolved = resolveEmbeddedCanvasSqliteUrl({
    env: { SDKWORK_DATABASE_SQLITE_URL: 'sqlite::memory:' },
    runtimeStateDirectory: '/tmp/state',
  });
  assert.equal(resolved, 'sqlite::memory:');
});

test('embedded canvas SQLite store requires a runtime state directory', () => {
  assert.throws(
    () => resolveEmbeddedCanvasSqliteUrl({ runtimeStateDirectory: '  ' }),
    /runtime state directory is required/u,
  );
});

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

test('embedded dependencies never default a dependency base URL to the gateway origin', () => {
  // APPLICATION_GATEWAY_SPEC §2.3: this gateway composes CloudRouter
  // in-process, so the image generation module is wired through the injected
  // in-process dispatch adapter and no per-dependency HTTP base URL is set at
  // all — a default pointing at any listener (own or external) would describe
  // a deployment shape this gateway no longer has.
  const resolved = resolveEmbeddedDependencyEnv({
    SDKWORK_API_CLOUD_GATEWAY_BIND: '127.0.0.1:3900',
  });

  assert.equal(resolved.SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL, undefined);
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
  assert.equal(
    resolved.SDKWORK_CLOUDROUTER_INTERNAL_GATEWAY_SIGNING_SECRET,
    'sdkwork-cloudrouter-local-dev-secret-20260507',
  );
  assert.equal(resolved.SDKWORK_RTC_HYDRATE_TENANT_ID, '100001');
  assert.equal(resolved.SDKWORK_RTC_HYDRATE_ORGANIZATION_ID, '0');
  assert.equal(resolved.SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID, '0');
  assert.equal(resolved.SDKWORK_BIRDCODER_ENVIRONMENT, 'development');
  assert.equal(resolved.SDKWORK_BIRDCODER_DEPLOYMENT_PROFILE, 'standalone');
  assert.equal(resolved.SDKWORK_BIRDCODER_RUNTIME_TARGET, 'server');
  assert.equal(resolved.SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT, 'development');
});

test('embedded cloudrouter router environment preserves explicit runtime overrides', () => {
  const resolved = resolveEmbeddedDependencyEnv({
    SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT: 'staging',
  });
  assert.equal(resolved.SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT, 'staging');
});

test('embedded birdcoder assembly preserves explicit runtime overrides', () => {
  const resolved = resolveEmbeddedDependencyEnv({
    SDKWORK_BIRDCODER_ENVIRONMENT: 'demo',
    SDKWORK_BIRDCODER_DEPLOYMENT_PROFILE: 'cloud',
    SDKWORK_BIRDCODER_RUNTIME_TARGET: 'container',
  });
  assert.equal(resolved.SDKWORK_BIRDCODER_ENVIRONMENT, 'demo');
  assert.equal(resolved.SDKWORK_BIRDCODER_DEPLOYMENT_PROFILE, 'cloud');
  assert.equal(resolved.SDKWORK_BIRDCODER_RUNTIME_TARGET, 'container');
});

test('embedded knowledgebase runtime defaults to the development tenant', () => {
  // The runtime rejects principals from another tenant with 40303, so a
  // development launch must scope it to the same tenant the IAM app logs in
  // with (agent.app.config.json backend.tenantId).
  const resolved = resolveEmbeddedDependencyEnv({});
  assert.equal(resolved.SDKWORK_KNOWLEDGEBASE_TENANT_ID, '100001');
  assert.equal(resolved.SDKWORK_KNOWLEDGEBASE_ENVIRONMENT, undefined);
});

test('embedded knowledgebase tenant preserves explicit overrides', () => {
  const resolved = resolveEmbeddedDependencyEnv({
    SDKWORK_KNOWLEDGEBASE_TENANT_ID: '1',
  });
  assert.equal(resolved.SDKWORK_KNOWLEDGEBASE_TENANT_ID, '1');
});

test('embedded webserver assembly preserves an explicit snowflake node id', () => {
  const resolved = resolveEmbeddedDependencyEnv({
    SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID: '7',
  });
  assert.equal(resolved.SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID, '7');
});

test('embedded modules receive the selected lifecycle environment', () => {
  const resolved = resolveEmbeddedDependencyEnv({
    SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT: 'development',
  });
  assert.equal(resolved.SDKWORK_IM_ENVIRONMENT, 'development');
  assert.equal(resolved.SDKWORK_KNOWLEDGEBASE_ENVIRONMENT, 'development');
  assert.equal(resolved.SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT, 'development');
  assert.equal(resolved.SDKWORK_MEMORY_ENVIRONMENT, 'development');
});

test('embedded memory keeps an explicit lifecycle environment over the gateway one', () => {
  const resolved = resolveEmbeddedDependencyEnv({
    SDKWORK_ENVIRONMENT: 'development',
    SDKWORK_MEMORY_ENVIRONMENT: 'staging',
  });

  assert.equal(resolved.SDKWORK_MEMORY_ENVIRONMENT, 'staging');
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

test('embedded dependency env never forwards a CloudRouter HTTP base URL', () => {
  // APPLICATION_GATEWAY_SPEC §2.3: a collapsed gateway that embeds CloudRouter
  // wires the image generation module through the in-process dispatch adapter,
  // so a per-dependency base URL must neither be defaulted nor forwarded —
  // leaving one set would describe the standalone-CloudRouter shape that this
  // deployment explicitly does not use.
  const explicit = resolveEmbeddedDependencyEnv({
    SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL: ' https://cloudrouter.example.com ',
    CLOUDROUTER_OPEN_API_BASE_URL: 'https://legacy.example.com',
    SDKWORK_CLOUDROUTER_API_KEY_PEPPER: 'explicit-api-key-pepper',
    SDKWORK_CLOUDROUTER_TRUSTED_SUBJECT_SECRET: 'explicit-trusted-subject-secret',
    SDKWORK_CLOUDROUTER_APP_SESSION_SECRET: 'explicit-app-session-secret',
    SDKWORK_CLOUDROUTER_PAYMENT_WEBHOOK_SECRET: 'explicit-payment-webhook-secret',
  });

  assert.equal(explicit.SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL, undefined);
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
    undefined,
  );
});

test('embedded dependency env ignores the gateway bind when deriving dependency origins', () => {
  // The gateway bind describes *this* listener. Deriving a dependency base URL
  // from it is the loopback self-forwarding that APPLICATION_GATEWAY_SPEC §2.3
  // prohibits, so a custom bind must not leak into the dependency origin.
  const resolved = resolveEmbeddedDependencyEnv({
    SDKWORK_API_CLOUD_GATEWAY_BIND: '0.0.0.0:4900',
  });

  assert.equal(resolved.SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL, undefined);
});

test('embedded memory exports default into the private runtime state directory', () => {
  const resolved = resolveEmbeddedMemoryObjectStoreRoot({
    env: {},
    runtimeStateDirectory: 'C:\\state\\sdkwork-api-cloud-gateway',
  });

  assert.equal(
    resolved,
    path.join('C:\\state\\sdkwork-api-cloud-gateway', 'memory-drive-objects'),
  );
});

test('embedded memory exports preserve an explicit object store root', () => {
  assert.equal(
    resolveEmbeddedMemoryObjectStoreRoot({
      env: { SDKWORK_MEMORY_DRIVE_OBJECT_STORE_ROOT: 'D:\\objects' },
      runtimeStateDirectory: 'C:\\state',
    }),
    'D:\\objects',
  );
});

test('embedded cloudrouter reuses the shared workspace development key ring', () => {
  // The upstream credentials in the shared development database were encrypted
  // by CloudRouter's own development ring, so a different inline ring fails at
  // startup with "catalog row mapping failed: failed to decrypt".
  const sharedRing = JSON.stringify({
    activeKeyId: 'development-local-v1',
    activeKey: 'shared-ring-key',
    fingerprintKey: 'shared-ring-fingerprint',
    decryptionKeys: [],
  });
  const resolved = resolveCloudRouterCredentialKeyRing({
    env: {},
    fileExists: (filePath) => filePath.endsWith(
      'upstream-credential-key-ring.development.json',
    ),
    readFile: () => sharedRing,
    repoRoot: 'D:\\workspace\\sdkwork-api-cloud-gateway',
  });

  assert.equal(resolved, sharedRing);
});

test('embedded cloudrouter falls back to the inline ring without the shared file', () => {
  const resolved = resolveCloudRouterCredentialKeyRing({
    env: {},
    fileExists: () => false,
    repoRoot: 'D:\\workspace\\sdkwork-api-cloud-gateway',
  });
  const parsed = JSON.parse(resolved);

  assert.equal(parsed.activeKeyId, 'development-local-v1');
  assert.match(parsed.activeKey, /^[A-Za-z0-9_-]{32,}$/u);
});

test('an explicit cloudrouter key ring is never overridden', () => {
  const explicit = '{"activeKeyId":"explicit","activeKey":"explicit-key"}';
  assert.equal(
    resolveCloudRouterCredentialKeyRing({
      env: { SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING: explicit },
      fileExists: () => true,
      repoRoot: 'D:\\workspace\\sdkwork-api-cloud-gateway',
    }),
    undefined,
  );
  assert.equal(
    resolveCloudRouterCredentialKeyRing({
      env: {
        SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING_FILE: 'D:\\ring.json',
      },
      fileExists: () => true,
      repoRoot: 'D:\\workspace\\sdkwork-api-cloud-gateway',
    }),
    undefined,
  );
});

test('a malformed shared key ring falls back instead of breaking startup', () => {
  const resolved = resolveCloudRouterCredentialKeyRing({
    env: {},
    fileExists: () => true,
    readFile: () => 'not json',
    repoRoot: 'D:\\workspace\\sdkwork-api-cloud-gateway',
  });

  assert.equal(JSON.parse(resolved).activeKeyId, 'development-local-v1');
});

test('the reported key-ring source names the shared ring when it is used', () => {
  assert.match(
    describeCloudRouterCredentialKeyRingSource({
      env: {},
      fileExists: () => true,
      repoRoot: 'D:\\workspace\\sdkwork-api-cloud-gateway',
    }),
    /shared CloudRouter development key ring/u,
  );
  assert.match(
    describeCloudRouterCredentialKeyRingSource({
      env: {},
      fileExists: () => false,
      repoRoot: 'D:\\workspace\\sdkwork-api-cloud-gateway',
    }),
    /inline development fallback/u,
  );
});

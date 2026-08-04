const cloudRouterDevelopmentSecret =
  'sdkwork-cloudrouter-local-dev-secret-20260507';

export function resolveTemporaryDatabaseDriverEnv(env = process.env) {
  return {
    SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION:
      env.SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION ?? 'true',
    SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT:
      env.SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT ?? '2',
  };
}

export function resolveEmbeddedPostgresUrl(databaseUrl, schema) {
  if (!firstNonEmpty(schema)) {
    throw new Error('embedded database schema is required');
  }
  const url = new URL(databaseUrl);
  // Keep -c attached: URLSearchParams encodes spaces as '+', which rust-postgres treats literally.
  url.searchParams.set('options', `-csearch_path=${schema}`);
  return url.toString();
}

export function resolveEmbeddedDatabaseEnv({ databaseUrl, schema, env = process.env }) {
  if (!firstNonEmpty(databaseUrl)) {
    throw new Error('embedded database URL is required');
  }
  if (!firstNonEmpty(schema)) {
    throw new Error('embedded database schema is required');
  }

  return {
    SDKWORK_DATABASE_URL: databaseUrl,
    SDKWORK_DATABASE_SCHEMA: schema,
    SDKWORK_AIOT_DEVICE_STORAGE_TABLE_PREFIX:
      env.SDKWORK_AIOT_DEVICE_STORAGE_TABLE_PREFIX ?? 'iot_',
    SDKWORK_IM_ID_NODE_ID: env.SDKWORK_IM_ID_NODE_ID ?? '0',
    SDKWORK_RTC_STATE_REQUIRE_DURABLE: 'true',
    SDKWORK_RTC_REQUIRE_OUTBOX: 'true',
  };
}

export function resolveEmbeddedDependencyEnv(env = process.env) {
  const configuredCloudRouterBaseUrl = firstNonEmpty(
    env.SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL,
    env.CLOUDROUTER_OPEN_API_BASE_URL,
  );
  const environment = firstNonEmpty(
    env.SDKWORK_ENVIRONMENT,
    env.SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT,
  );
  const imEnvironment = firstNonEmpty(env.SDKWORK_IM_ENVIRONMENT, environment);
  const knowledgebaseEnvironment = firstNonEmpty(
    env.SDKWORK_KNOWLEDGEBASE_ENVIRONMENT,
    environment,
  );

  return {
    // The generated CloudRouter Rust SDK adds its own /v1 prefix.
    SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL:
      configuredCloudRouterBaseUrl ?? localGatewayOrigin(env.SDKWORK_API_CLOUD_GATEWAY_BIND),
    SDKWORK_CLOUDROUTER_API_KEY_PEPPER:
      env.SDKWORK_CLOUDROUTER_API_KEY_PEPPER ?? cloudRouterDevelopmentSecret,
    SDKWORK_CLOUDROUTER_TRUSTED_SUBJECT_SECRET:
      env.SDKWORK_CLOUDROUTER_TRUSTED_SUBJECT_SECRET ?? cloudRouterDevelopmentSecret,
    SDKWORK_CLOUDROUTER_APP_SESSION_SECRET:
      env.SDKWORK_CLOUDROUTER_APP_SESSION_SECRET ?? cloudRouterDevelopmentSecret,
    SDKWORK_CLOUDROUTER_PAYMENT_WEBHOOK_SECRET:
      env.SDKWORK_CLOUDROUTER_PAYMENT_WEBHOOK_SECRET ?? cloudRouterDevelopmentSecret,
    ...(imEnvironment ? { SDKWORK_IM_ENVIRONMENT: imEnvironment } : {}),
    ...(knowledgebaseEnvironment
      ? { SDKWORK_KNOWLEDGEBASE_ENVIRONMENT: knowledgebaseEnvironment }
      : {}),
  };
}

function localGatewayOrigin(configuredBind) {
  const bind = firstNonEmpty(configuredBind) ?? '127.0.0.1:3900';
  let url;
  try {
    url = new URL(bind.includes('://') ? bind : `http://${bind}`);
  } catch {
    return 'http://127.0.0.1:3900';
  }

  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    return 'http://127.0.0.1:3900';
  }
  if (url.hostname === '0.0.0.0') {
    url.hostname = '127.0.0.1';
  } else if (url.hostname === '[::]') {
    url.hostname = '[::1]';
  }
  return url.origin;
}

function firstNonEmpty(...values) {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
  }
  return undefined;
}

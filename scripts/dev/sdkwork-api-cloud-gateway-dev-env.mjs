const embeddedDatabaseServices = [
  'AIOT_DEVICE',
  'DRIVE',
  'ORDER',
  'PROMOTION',
  'PAYMENT',
  'IAM',
  'IM',
  'KNOWLEDGEBASE',
  'AGENTS',
  'AGENTS_STORE',
  'AGENT_RUNTIME',
  'DEPLOY',
  'DEZHOU',
  'DOCUMENTS',
  'MODELKIT',
  'PROMPTS',
  'WEB',
];

const clawRouterDevelopmentSecret =
  'sdkwork-clawrouter-local-dev-secret-20260507';

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

  const resolved = {};
  for (const service of embeddedDatabaseServices) {
    resolved[`SDKWORK_${service}_DATABASE_URL`] = databaseUrl;
    resolved[`SDKWORK_${service}_DATABASE_SCHEMA`] = schema;
  }
  resolved.SDKWORK_AIOT_DEVICE_DATABASE_TABLE_PREFIX =
    env.SDKWORK_AIOT_DEVICE_DATABASE_TABLE_PREFIX ?? 'iot_';
  resolved.SDKWORK_IM_ID_NODE_ID = env.SDKWORK_IM_ID_NODE_ID ?? '0';
  resolved.SDKWORK_RTC_STATE_DATABASE_URL = databaseUrl;
  resolved.SDKWORK_RTC_OUTBOX_DATABASE_URL = databaseUrl;
  resolved.SDKWORK_RTC_STATE_REQUIRE_DURABLE = 'true';
  resolved.SDKWORK_RTC_REQUIRE_OUTBOX = 'true';
  return resolved;
}

export function resolveEmbeddedDependencyEnv(env = process.env) {
  const configuredClawRouterBaseUrl = firstNonEmpty(
    env.SDKWORK_CLAWROUTER_OPEN_API_BASE_URL,
    env.CLAWROUTER_OPEN_API_BASE_URL,
  );
  const environment = firstNonEmpty(
    env.SDKWORK_IM_ENVIRONMENT,
    env.SDKWORK_ENVIRONMENT,
    env.SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT,
  );

  return {
    // The generated ClawRouter Rust SDK adds its own /v1 prefix.
    SDKWORK_CLAWROUTER_OPEN_API_BASE_URL:
      configuredClawRouterBaseUrl ?? localGatewayOrigin(env.SDKWORK_API_CLOUD_GATEWAY_BIND),
    SDKWORK_CLAW_API_KEY_PEPPER:
      env.SDKWORK_CLAW_API_KEY_PEPPER ?? clawRouterDevelopmentSecret,
    SDKWORK_CLAW_TRUSTED_SUBJECT_SECRET:
      env.SDKWORK_CLAW_TRUSTED_SUBJECT_SECRET ?? clawRouterDevelopmentSecret,
    SDKWORK_CLAW_APP_SESSION_SECRET:
      env.SDKWORK_CLAW_APP_SESSION_SECRET ?? clawRouterDevelopmentSecret,
    SDKWORK_CLAW_PAYMENT_WEBHOOK_SECRET:
      env.SDKWORK_CLAW_PAYMENT_WEBHOOK_SECRET ?? clawRouterDevelopmentSecret,
    ...(environment ? { SDKWORK_IM_ENVIRONMENT: environment } : {}),
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

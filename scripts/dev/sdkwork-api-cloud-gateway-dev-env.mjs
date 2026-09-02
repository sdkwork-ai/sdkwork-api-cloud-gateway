const cloudRouterDevelopmentSecret =
  'sdkwork-cloudrouter-local-dev-secret-20260507';

// Local-development upstream credential key ring. Mirrors
// docker/env/development.env (GATEWAY_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING)
// so the embedded standalone dev deployment can decrypt the upstream
// credentials that the docker dev stack already provisioned into the shared
// sdkwork_ai_dev database; a mismatched activeKeyId is what produces
// "credential secret key id is unavailable" when the cloudrouter router build
// maps the catalog. Required by the cloudrouter embedded routers because
// PostgreSQL upstream routing encrypts stored upstream credentials with it.
const cloudRouterDevelopmentKeyRing = JSON.stringify({
  activeKeyId: 'development-local-v1',
  activeKey: '-H9WLZu6Ou7TZHIOSrl5axiRAK10KOkjrcFbYnWZabk',
  fingerprintKey: 'HiYnoe11mwTzAyCWK0JVfhLahiVMRvZNi_IrxaZgh5o',
  decryptionKeys: [],
});

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
    // APPLICATION_GATEWAY_SPEC §2.3: this gateway composes the CloudRouter
    // assembly in-process, so the image generation module reaches CloudRouter
    // through the injected in-process dispatch adapter. A per-dependency HTTP
    // base URL (`SDKWORK_CLOUDROUTER_OPEN_API_BASE_URL`) describes only a
    // separately deployed CloudRouter and must not be set for this collapsed
    // deployment shape, so it is deliberately not defaulted or forwarded here.
    SDKWORK_CLOUDROUTER_API_KEY_PEPPER:
      env.SDKWORK_CLOUDROUTER_API_KEY_PEPPER ?? cloudRouterDevelopmentSecret,
    SDKWORK_CLOUDROUTER_TRUSTED_SUBJECT_SECRET:
      env.SDKWORK_CLOUDROUTER_TRUSTED_SUBJECT_SECRET ?? cloudRouterDevelopmentSecret,
    SDKWORK_CLOUDROUTER_APP_SESSION_SECRET:
      env.SDKWORK_CLOUDROUTER_APP_SESSION_SECRET ?? cloudRouterDevelopmentSecret,
    SDKWORK_CLOUDROUTER_PAYMENT_WEBHOOK_SECRET:
      env.SDKWORK_CLOUDROUTER_PAYMENT_WEBHOOK_SECRET ?? cloudRouterDevelopmentSecret,
    // Required by the CloudRouter embedded router build: app runtime gateway
    // calls are signed/verified with this secret, so it must be present even
    // in the collapsed standalone development deployment.
    SDKWORK_CLOUDROUTER_INTERNAL_GATEWAY_SIGNING_SECRET:
      env.SDKWORK_CLOUDROUTER_INTERNAL_GATEWAY_SIGNING_SECRET
      ?? cloudRouterDevelopmentSecret,
    // RTC assembly startup hydrates runtime state from persistence using this
    // tenant/organization scope; without it the assembly falls back to the
    // "default" placeholder, which fails to parse as a tenant id. Mirrors the
    // docker-compose development defaults (GATEWAY_RTC_HYDRATE_*).
    SDKWORK_RTC_HYDRATE_TENANT_ID: env.SDKWORK_RTC_HYDRATE_TENANT_ID ?? '100001',
    SDKWORK_RTC_HYDRATE_ORGANIZATION_ID:
      env.SDKWORK_RTC_HYDRATE_ORGANIZATION_ID ?? '0',
    ...(!firstNonEmpty(
      env.SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING,
      env.SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING_FILE,
    )
      ? {
          SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING:
            cloudRouterDevelopmentKeyRing,
        }
      : {}),
    ...(imEnvironment ? { SDKWORK_IM_ENVIRONMENT: imEnvironment } : {}),
    ...(knowledgebaseEnvironment
      ? { SDKWORK_KNOWLEDGEBASE_ENVIRONMENT: knowledgebaseEnvironment }
      : {}),
  };
}

function firstNonEmpty(...values) {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
  }
  return undefined;
}

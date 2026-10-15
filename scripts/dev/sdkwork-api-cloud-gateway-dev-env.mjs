import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const cloudRouterDevelopmentSecret =
  'sdkwork-cloudrouter-local-dev-secret-20260507';

// Fallback local-development upstream credential key ring. The router keys must
// match whatever encrypted the credentials already stored in the shared
// development database; CloudRouter owns that key ring and writes it to its own
// ignored secrets directory (`scripts/generate-dev-secrets.mjs`). A mismatch
// fails at startup with "catalog row mapping failed: failed to decrypt
// credential secret", so the shared ring is preferred whenever it is findable
// and this inline value is only the last resort.
const cloudRouterDevelopmentKeyRing = JSON.stringify({
  activeKeyId: 'development-local-v1',
  activeKey: '-H9WLZu6Ou7TZHIOSrl5axiRAK10KOkjrcFbYnWZabk',
  fingerprintKey: 'HiYnoe11mwTzAyCWK0JVfhLahiVMRvZNi_IrxaZgh5o',
  decryptionKeys: [],
});

// CloudRouter's ignored development key ring, resolved as a sibling checkout of
// this repository inside the active workspace.
const cloudRouterDevelopmentKeyRingRelativePath = path.join(
  'sdkwork-cloudrouter',
  '.sdkwork',
  'secrets',
  'upstream-credential-key-ring.development.json',
);

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

export function resolveEmbeddedCanvasSqliteUrl({
  runtimeStateDirectory,
  env = process.env,
}) {
  if (!firstNonEmpty(runtimeStateDirectory)) {
    throw new Error('embedded canvas runtime state directory is required');
  }
  // Canvas pages are SQLite-only (ENVIRONMENT_SPEC §7.2): the embedded gateway
  // deployment needs a client-local SQLite store next to the server runtime
  // state. Mirrors the docker-compose default (GATEWAY_DATABASE_SQLITE_URL),
  // which stores canvas-pages.db under the container data volume.
  const existing = firstNonEmpty(env.SDKWORK_DATABASE_SQLITE_URL);
  if (existing) {
    return existing;
  }
  const sqlitePath = path
    .join(runtimeStateDirectory, 'canvas-pages.db')
    .replace(/\\/gu, '/');
  return `sqlite:${sqlitePath}?mode=rwc`;
}

/**
 * Resolve the local object-store root used by the embedded memory assembly's
 * Drive-backed exports.
 *
 * The assembly fails its router build with "SDKWORK_MEMORY_DRIVE_OBJECT_STORE_ROOT
 * is required for local filesystem drive export" when no root is configured.
 * Exported objects are disposable runtime state, so the development default
 * lives under the private runtime state directory
 * (`RUNTIME_DIRECTORY_SPEC.md`), never inside the source tree.
 */
export function resolveEmbeddedMemoryObjectStoreRoot({
  runtimeStateDirectory,
  env = process.env,
}) {
  if (!firstNonEmpty(runtimeStateDirectory)) {
    throw new Error('embedded memory runtime state directory is required');
  }
  const existing = firstNonEmpty(env.SDKWORK_MEMORY_DRIVE_OBJECT_STORE_ROOT);
  if (existing) {
    return existing;
  }
  return path.join(runtimeStateDirectory, 'memory-drive-objects');
}

export function resolveEmbeddedDependencyEnv(
  env = process.env,
  {
    repoRoot = defaultRepoRoot(),
    fileExists = existsSync,
    readFile = (filePath) => readFileSync(filePath, 'utf8'),
  } = {},
) {
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
  // The Memory server runtime refuses to start without an explicit lifecycle
  // environment: the historical implicit 'development' default silently
  // disabled authorization, tenant isolation, Redis-backed rate limiting, and
  // request deadlines.
  const memoryEnvironment = firstNonEmpty(env.SDKWORK_MEMORY_ENVIRONMENT, environment);
  const credentialKeyRing = resolveCloudRouterCredentialKeyRing({
    env,
    repoRoot,
    fileExists,
    readFile,
  });

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
    // Required by the CloudRouter embedded router build: the outbound target
    // policy derives from the router environment, and parse_environment fails
    // closed to the production posture when it is unset. Local development
    // runs behind fake-IP proxies (Clash TUN 198.18.0.0/15), so an unset value
    // makes every provider hop fail with "forbidden address" (50201). Same
    // precedence as the BirdCoder lifecycle environment: explicit override,
    // then the selected gateway environment, then development.
    SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT:
      env.SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT ?? environment ?? 'development',
    // Required by the sdkwork-webserver embedded assembly (Web Server API
    // router build): the snowflake id generator needs a node id. Standalone
    // development runs a single gateway instance, so 0 is a safe default;
    // multi-instance deployments must assign unique ids per replica.
    SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID:
      env.SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID ?? '0',
    // Required by the sdkwork-birdcoder embedded assembly: the BirdCoder
    // server runtime refuses to boot without an explicit lifecycle
    // environment, deployment profile, and runtime target. The collapsed
    // gateway dev deployment is a single standalone server process that
    // inherits the selected gateway lifecycle environment.
    SDKWORK_BIRDCODER_ENVIRONMENT:
      env.SDKWORK_BIRDCODER_ENVIRONMENT ?? environment ?? 'development',
    SDKWORK_BIRDCODER_DEPLOYMENT_PROFILE:
      env.SDKWORK_BIRDCODER_DEPLOYMENT_PROFILE ?? 'standalone',
    SDKWORK_BIRDCODER_RUNTIME_TARGET:
      env.SDKWORK_BIRDCODER_RUNTIME_TARGET ?? 'server',
    // RTC assembly startup hydrates runtime state from persistence using this
    // tenant/organization scope; without it the assembly falls back to the
    // "default" placeholder, which fails to parse as a tenant id. Mirrors the
    // docker-compose development defaults (GATEWAY_RTC_HYDRATE_*).
    SDKWORK_RTC_HYDRATE_TENANT_ID: env.SDKWORK_RTC_HYDRATE_TENANT_ID ?? '100001',
    SDKWORK_RTC_HYDRATE_ORGANIZATION_ID:
      env.SDKWORK_RTC_HYDRATE_ORGANIZATION_ID ?? '0',
    ...(credentialKeyRing
      ? { SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING: credentialKeyRing }
      : {}),
    ...(imEnvironment ? { SDKWORK_IM_ENVIRONMENT: imEnvironment } : {}),
    ...(knowledgebaseEnvironment
      ? { SDKWORK_KNOWLEDGEBASE_ENVIRONMENT: knowledgebaseEnvironment }
      : {}),
    ...(memoryEnvironment ? { SDKWORK_MEMORY_ENVIRONMENT: memoryEnvironment } : {}),
  };
}

/**
 * Resolve the upstream credential key ring for the embedded CloudRouter.
 *
 * An explicit `SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING(_FILE)` always
 * wins. Otherwise the shared CloudRouter development ring is read from the
 * sibling checkout so both processes decrypt the same rows, and the inline
 * development ring is used only when that file cannot be read.
 *
 * @returns {string | undefined} Key-ring JSON, or undefined when the caller
 *   already configured one.
 */
export function resolveCloudRouterCredentialKeyRing({
  env = process.env,
  repoRoot = defaultRepoRoot(),
  fileExists = existsSync,
  readFile = (filePath) => readFileSync(filePath, 'utf8'),
} = {}) {
  if (
    firstNonEmpty(
      env.SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING,
      env.SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING_FILE,
    )
  ) {
    return undefined;
  }
  const sharedKeyRingPath = path.join(
    repoRoot,
    '..',
    cloudRouterDevelopmentKeyRingRelativePath,
  );
  if (fileExists(sharedKeyRingPath)) {
    try {
      const parsed = JSON.parse(readFile(sharedKeyRingPath));
      if (firstNonEmpty(parsed?.activeKeyId) && firstNonEmpty(parsed?.activeKey)) {
        return JSON.stringify(parsed);
      }
    } catch {
      // Fall through to the inline development ring below; the caller reports
      // which ring was selected.
    }
  }
  return cloudRouterDevelopmentKeyRing;
}

/**
 * Report which key ring the embedded CloudRouter will use, so a decryption
 * failure at startup names its cause instead of only the symptom.
 */
export function describeCloudRouterCredentialKeyRingSource({
  env = process.env,
  repoRoot = defaultRepoRoot(),
  fileExists = existsSync,
} = {}) {
  if (firstNonEmpty(env.SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING)) {
    return 'explicit environment key ring';
  }
  if (firstNonEmpty(env.SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING_FILE)) {
    return `key ring file ${firstNonEmpty(env.SDKWORK_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING_FILE)}`;
  }
  const sharedKeyRingPath = path.join(
    repoRoot,
    '..',
    cloudRouterDevelopmentKeyRingRelativePath,
  );
  return fileExists(sharedKeyRingPath)
    ? `shared CloudRouter development key ring (${sharedKeyRingPath})`
    : 'inline development fallback key ring (shared CloudRouter ring not found)';
}

function defaultRepoRoot() {
  return path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
}

function firstNonEmpty(...values) {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
  }
  return undefined;
}

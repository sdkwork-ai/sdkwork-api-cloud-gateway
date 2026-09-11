#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

import { parseDotEnv } from '../../sdkwork-specs/tools/postgres/postgres-config.mjs';
import {
  DATABASE_IDENTITIES,
  ENVIRONMENTS,
  ENVIRONMENT_SUFFIX,
} from './deployment-environments.mjs';
import {
  BASE_DOMAINS,
  canonicalConsoleHostConfig,
  canonicalSeedOrigins,
  isPatternCoveredOrigin,
} from './cors-allowed-origins.mjs';

// Re-exported so downstream consumers (contract tests, packagers) keep a
// single import surface while the canonical list lives in the shared registry.
export {
  DATABASE_IDENTITIES,
  ENVIRONMENTS,
  ENVIRONMENT_SUFFIX,
};

const DESKTOP_CORS_ORIGINS = [
  'app://dsh',
  'app://birdcoder',
  'app://sdkwork',
  'app://dtupay',
  'tauri://localhost',
];
const MINI_PROGRAM_CORS_ORIGINS = ['https://servicewechat.com'];
const DEPENDENCY_MODES = ['embedded', 'external'];
const PRODUCTION_SECRET_KEYS = [
  'GATEWAY_CLOUDROUTER_API_KEY_PEPPER',
  'GATEWAY_CLOUDROUTER_UPSTREAM_CREDENTIAL_KEY_RING',
  'GATEWAY_CLOUDROUTER_TRUSTED_SUBJECT_SECRET',
  'GATEWAY_CLOUDROUTER_APP_SESSION_SECRET',
  'GATEWAY_CLOUDROUTER_PAYMENT_WEBHOOK_SECRET',
  'GATEWAY_CLOUDROUTER_INTERNAL_GATEWAY_SIGNING_SECRET',
];

function expectedHosts(environment) {
  const suffix = ENVIRONMENT_SUFFIX[environment];
  if (suffix === undefined) {
    throw new Error(`unsupported lifecycle environment: ${environment}`);
  }
  const role = suffix ? `api-${suffix}` : 'api';
  return BASE_DOMAINS.map((domain) => `${role}.${domain}`);
}

function csv(value) {
  return String(value ?? '')
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function requireResolved(env, key) {
  const value = String(env[key] ?? '').trim();
  if (!value || /<[^>]+>|change-me/iu.test(value)) {
    throw new Error(`${key} must be set to a resolved non-placeholder value`);
  }
  return value;
}

function validateDatabaseUrlInputs(env) {
  const explicitUrl = String(env.SDKWORK_DATABASE_URL ?? '').trim();
  if (explicitUrl) {
    requireResolved(env, 'SDKWORK_DATABASE_URL');
    let parsed;
    try {
      parsed = new URL(explicitUrl);
    } catch {
      throw new Error('SDKWORK_DATABASE_URL must be a valid URL');
    }
    if (parsed.protocol !== 'postgresql:' && parsed.protocol !== 'postgres:') {
      throw new Error('SDKWORK_DATABASE_URL must use postgresql:// or postgres://');
    }
    return;
  }

  if (env.GATEWAY_POSTGRES_PASSWORD_FILE) {
    throw new Error(
      'SDKWORK_DATABASE_URL is required when GATEWAY_POSTGRES_PASSWORD_FILE is used',
    );
  }
  for (const key of [
    'GATEWAY_POSTGRES_DB',
    'GATEWAY_POSTGRES_SCHEMA',
    'GATEWAY_POSTGRES_USER',
    'GATEWAY_POSTGRES_PASSWORD',
  ]) {
    const value = requireResolved(env, key);
    if (!/^[A-Za-z0-9._~-]+$/u.test(value)) {
      throw new Error(
        `${key} contains URL-reserved characters; set an encoded SDKWORK_DATABASE_URL`,
      );
    }
  }
}

export function validateDeploymentEnvironment(env, mode) {
  if (!['embedded', 'external'].includes(mode)) {
    throw new Error(`unsupported dependency mode: ${mode}`);
  }
  const environment = requireResolved(env, 'GATEWAY_ENVIRONMENT');
  const expectedProfileId = `standalone.${environment}`;
  if (env.GATEWAY_PROFILE_ID !== expectedProfileId) {
    throw new Error(`GATEWAY_PROFILE_ID must be ${expectedProfileId}`);
  }
  requireResolved(env, 'GATEWAY_IMAGE');
  const database = requireResolved(env, 'GATEWAY_POSTGRES_DB');
  const schema = requireResolved(env, 'GATEWAY_POSTGRES_SCHEMA');
  const username = requireResolved(env, 'GATEWAY_POSTGRES_USER');
  const expectedDatabaseIdentity = DATABASE_IDENTITIES[environment];
  if (!expectedDatabaseIdentity) {
    throw new Error(`unsupported database environment: ${environment}`);
  }
  for (const [key, value] of [
    ['GATEWAY_POSTGRES_DB', database],
    ['GATEWAY_POSTGRES_SCHEMA', schema],
    ['GATEWAY_POSTGRES_USER', username],
  ]) {
    if (value !== expectedDatabaseIdentity) {
      throw new Error(`${key} must be ${expectedDatabaseIdentity}`);
    }
  }
  validateDatabaseUrlInputs(env);

  const hosts = expectedHosts(environment);
  const allowedHosts = csv(env.GATEWAY_ALLOWED_HOSTS);
  for (const host of hosts) {
    if (!allowedHosts.includes(host)) {
      throw new Error(`GATEWAY_ALLOWED_HOSTS is missing ${host}`);
    }
  }
  // Browser CORS is declared as a *pattern*, not an enumeration: the console
  // closure is `label x base domain x suffix x scheme` and enumerating it costs
  // ~3000 exact origins (~100 KiB) per non-production environment, which is a
  // deployment hazard against the Linux 128 KiB `MAX_ARG_STRLEN` limit. The
  // gateway matches the pattern exactly per component (sdkwork-web-core
  // `registered_console_hosts`), so the carriers must declare the pattern and
  // must NOT re-enumerate the closure.
  const corsOrigins = csv(env.GATEWAY_CORS_ALLOWED_ORIGINS);
  for (const origin of corsOrigins) {
    if (origin.includes('*')) {
      throw new Error(`GATEWAY_CORS_ALLOWED_ORIGINS contains a wildcard origin: ${origin}`);
    }
    if (!/^(https?|app|tauri):\/\//u.test(origin)) {
      throw new Error(`GATEWAY_CORS_ALLOWED_ORIGINS contains an unsupported origin: ${origin}`);
    }
    if (isPatternCoveredOrigin(origin, environment)) {
      throw new Error(
        `GATEWAY_CORS_ALLOWED_ORIGINS enumerates the console pattern (${origin}); ` +
          'the registered console host pattern already covers it — keep only ' +
          'the exact seeds (run: node scripts/derive-cors-allowed-origins.mjs --apply)',
      );
    }
  }
  const expectedSeeds = canonicalSeedOrigins(environment);
  const missingSeeds = expectedSeeds.filter((origin) => !corsOrigins.includes(origin));
  if (missingSeeds.length > 0) {
    throw new Error(
      `GATEWAY_CORS_ALLOWED_ORIGINS is missing the loopback seed(s) ${missingSeeds.join(', ')} ` +
        '(run: node scripts/derive-cors-allowed-origins.mjs --apply)',
    );
  }
  // Registered desktop-shell and mini-program origins are merged into every
  // policy by the web framework, so a carrier that repeats them only invites
  // drift; flag the regression instead of silently accepting it.
  for (const origin of [...DESKTOP_CORS_ORIGINS, ...MINI_PROGRAM_CORS_ORIGINS]) {
    if (corsOrigins.includes(origin)) {
      throw new Error(
        `GATEWAY_CORS_ALLOWED_ORIGINS repeats ${origin}, which the web framework merges ` +
          'into every policy (run: node scripts/derive-cors-allowed-origins.mjs --apply)',
      );
    }
  }

  const consoleConfig = canonicalConsoleHostConfig(environment);
  const declaredLabels = csv(env.GATEWAY_CORS_CONSOLE_HOST_LABELS);
  if (declaredLabels.length === 0) {
    throw new Error(
      'GATEWAY_CORS_CONSOLE_HOST_LABELS is required: the gateway matches Origin exactly and ' +
        'has no sub-domain wildcard, so every module console is rejected without the pattern',
    );
  }
  const missingLabels = consoleConfig.labels
    .split(',')
    .filter((label) => !declaredLabels.includes(label));
  if (missingLabels.length > 0) {
    throw new Error(
      `GATEWAY_CORS_CONSOLE_HOST_LABELS is missing ${missingLabels.length} registered label(s), ` +
        `e.g. ${missingLabels.slice(0, 3).join(', ')} ` +
        '(run: node scripts/derive-cors-allowed-origins.mjs --apply)',
    );
  }
  // The API plane host is itself a page origin in some environments, so the
  // `api` label is load-bearing for `GATEWAY_ALLOWED_HOSTS` coverage.
  if (!declaredLabels.includes('api')) {
    throw new Error('GATEWAY_CORS_CONSOLE_HOST_LABELS must include the `api` edge label');
  }
  if (env.GATEWAY_CORS_CONSOLE_HOST_SUFFIX !== consoleConfig.suffix) {
    throw new Error(
      `GATEWAY_CORS_CONSOLE_HOST_SUFFIX must be "${consoleConfig.suffix}" for ${environment} ` +
        `(got "${env.GATEWAY_CORS_CONSOLE_HOST_SUFFIX ?? ''}")`,
    );
  }
  const declaredSchemes = csv(env.GATEWAY_CORS_CONSOLE_HOST_SCHEMES);
  const missingSchemes = consoleConfig.schemes
    .split(',')
    .filter((scheme) => !declaredSchemes.includes(scheme));
  if (missingSchemes.length > 0) {
    throw new Error(
      `GATEWAY_CORS_CONSOLE_HOST_SCHEMES must include ${consoleConfig.schemes} for ${environment} ` +
        `(got "${env.GATEWAY_CORS_CONSOLE_HOST_SCHEMES ?? ''}")`,
    );
  }
  if (environment === 'production' && declaredSchemes.includes('http')) {
    throw new Error('GATEWAY_CORS_CONSOLE_HOST_SCHEMES must stay HTTPS-only in production');
  }
  if (environment !== 'production' && !declaredSchemes.includes('http')) {
    throw new Error(
      `GATEWAY_CORS_CONSOLE_HOST_SCHEMES must include http in ${environment}: the environment ` +
        'edge serves plain HTTP on its host port',
    );
  }

  if (mode === 'external') {
    requireResolved(env, 'GATEWAY_POSTGRES_HOST');
    requireResolved(env, 'GATEWAY_CLOUDROUTER_REDIS_HOST');
  }
  if (env.GATEWAY_IM_PRINCIPAL_DIRECTORY !== 'postgres') {
    throw new Error('GATEWAY_IM_PRINCIPAL_DIRECTORY must be postgres');
  }
  if (String(env.GATEWAY_IM_ALLOW_ALL_PRINCIPALS).toLowerCase() !== 'false') {
    throw new Error('GATEWAY_IM_ALLOW_ALL_PRINCIPALS must be false');
  }
  if (environment === 'production') {
    for (const key of PRODUCTION_SECRET_KEYS) {
      requireResolved(env, key);
    }
  }

  return { environment, expectedProfileId, hosts, mode };
}

function runComposeConfig(appRoot, envFile, mode, environmentOverrides = {}) {
  // External PostgreSQL/Redis is the default compose posture; the optional
  // docker-compose.external.yml shim is a no-op kept for operator habit.
  const args = ['compose', '--env-file', envFile, '-f', 'docker-compose.yml'];
  if (mode === 'embedded') {
    args.push('--profile', 'embedded', '-f', 'docker-compose.embedded.yml');
  }
  args.push('config', '--quiet');
  const result = spawnSync('docker', args, {
    cwd: appRoot,
    encoding: 'utf8',
    env: { ...process.env, ...environmentOverrides },
    shell: false,
  });
  if (result.error) {
    throw new Error(`docker compose is unavailable: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`docker compose config failed: ${result.stderr || result.stdout}`);
  }
}

export function resolveExampleEnvironment(env, mode) {
  const resolved = Object.fromEntries(
    Object.entries(env).map(([key, value]) => [
      key,
      String(value).replace(/<[^>]+>/gu, `validation-only-${key.toLowerCase()}`),
    ]),
  );
  if (mode === 'external') {
    resolved.GATEWAY_POSTGRES_HOST = 'postgres.validation.internal';
    resolved.GATEWAY_CLOUDROUTER_REDIS_HOST = 'redis.validation.internal';
    const redisHost = resolved.GATEWAY_CLOUDROUTER_REDIS_HOST;
    const redisPort = resolved.GATEWAY_CLOUDROUTER_REDIS_PORT || '6379';
    if (!resolved.GATEWAY_RTC_STATE_REDIS_URL) {
      resolved.GATEWAY_RTC_STATE_REDIS_URL = `redis://${redisHost}:${redisPort}/1`;
    }
    if (!resolved.GATEWAY_WEB_REDIS_URL) {
      resolved.GATEWAY_WEB_REDIS_URL = `redis://${redisHost}:${redisPort}/2`;
    }
  }
  return resolved;
}

export function validateDeploymentMatrix(appRoot, { compose = false } = {}) {
  const summaries = [];
  for (const environment of ENVIRONMENTS) {
    const envFile = path.join(appRoot, 'docker', 'env', `${environment}.env.example`);
    const source = parseDotEnv(readFileSync(envFile, 'utf8'));
    for (const mode of DEPENDENCY_MODES) {
      const resolved = resolveExampleEnvironment(source, mode);
      summaries.push(validateDeploymentEnvironment(resolved, mode));
      if (compose) {
        runComposeConfig(appRoot, envFile, mode, resolved);
      }
    }
  }
  return summaries;
}

function main() {
  const { values } = parseArgs({
    args: process.argv.slice(2),
    options: {
      compose: { type: 'boolean', default: false },
      'env-file': { type: 'string' },
      matrix: { type: 'boolean', default: false },
      mode: { type: 'string', default: 'embedded' },
    },
    strict: true,
  });
  const scriptPath = fileURLToPath(import.meta.url);
  const appRoot = path.resolve(path.dirname(scriptPath), '..');
  if (values.matrix) {
    if (values['env-file']) {
      throw new Error('--matrix and --env-file are mutually exclusive');
    }
    const summaries = validateDeploymentMatrix(appRoot, { compose: values.compose });
    process.stdout.write(`${JSON.stringify({ ok: true, count: summaries.length, summaries })}\n`);
    return;
  }
  if (!values['env-file']) {
    throw new Error('--env-file is required unless --matrix is selected');
  }
  const envFile = path.resolve(values['env-file']);
  const env = parseDotEnv(readFileSync(envFile, 'utf8'));
  const summary = validateDeploymentEnvironment(env, values.mode);
  if (values.compose) {
    runComposeConfig(appRoot, envFile, values.mode);
  }
  process.stdout.write(`${JSON.stringify({ ok: true, ...summary })}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}

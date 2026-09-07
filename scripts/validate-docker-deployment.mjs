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

// Re-exported so downstream consumers (contract tests, packagers) keep a
// single import surface while the canonical list lives in the shared registry.
export {
  DATABASE_IDENTITIES,
  ENVIRONMENTS,
  ENVIRONMENT_SUFFIX,
};

// Registered public base-domain family (ENVIRONMENT_SPEC §5.1.0.1): the edge
// serves api-<suffix>.<base-domain> for every entry. Keep aligned with
// scripts/deploy/lib/environments.sh and etc/sdkwork.deployment.config.json.
const BASE_DOMAINS = [
  'sdkwork.com', 'birdcoder.com', 'dtupay.com', 'noaper.com',
  'sdkwork.cn', 'birdcoder.cn', 'dtupay.cn', 'noaper.cn',
  'skubc.com', 'skubc.cn',
  'zowalk.com', 'zowalk.cn',
  'offer86.com', 'offer86.cn',
  '86offer.com', '86offer.cn',
];
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
  const corsOrigins = csv(env.GATEWAY_CORS_ALLOWED_ORIGINS);
  const corsHosts = corsOrigins.flatMap((origin) => {
    let parsed;
    try {
      parsed = new URL(origin);
    } catch {
      throw new Error(`GATEWAY_CORS_ALLOWED_ORIGINS contains an invalid origin: ${origin}`);
    }
    if (origin.includes('*')) {
      throw new Error(`GATEWAY_CORS_ALLOWED_ORIGINS contains a wildcard origin: ${origin}`);
    }
    if (parsed.protocol === 'http:' || parsed.protocol === 'https:') {
      return [parsed.hostname];
    }
    if (parsed.protocol === 'app:' || parsed.protocol === 'tauri:') {
      return [];
    }
    throw new Error(`GATEWAY_CORS_ALLOWED_ORIGINS contains an unsupported origin: ${origin}`);
  });
  for (const host of hosts) {
    if (!corsHosts.includes(host)) {
      throw new Error(`GATEWAY_CORS_ALLOWED_ORIGINS is missing ${host}`);
    }
  }
  for (const origin of DESKTOP_CORS_ORIGINS) {
    if (!corsOrigins.includes(origin)) {
      throw new Error(`GATEWAY_CORS_ALLOWED_ORIGINS is missing ${origin}`);
    }
  }
  for (const origin of MINI_PROGRAM_CORS_ORIGINS) {
    if (!corsOrigins.includes(origin)) {
      throw new Error(`GATEWAY_CORS_ALLOWED_ORIGINS is missing ${origin}`);
    }
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

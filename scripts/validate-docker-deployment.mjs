#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

import { parseDotEnv } from '../../sdkwork-specs/tools/postgres/postgres-config.mjs';

const BASE_DOMAINS = ['sdkwork.com', 'birdcoder.com', 'dtupay.com'];
const ENVIRONMENT_SUFFIX = {
  development: 'dev',
  test: 'test',
  staging: 'staging',
  production: '',
};
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
  requireResolved(env, 'GATEWAY_POSTGRES_USER');
  if (database !== schema) {
    throw new Error('GATEWAY_POSTGRES_SCHEMA must equal GATEWAY_POSTGRES_DB');
  }
  if (!env.GATEWAY_POSTGRES_PASSWORD_FILE) {
    requireResolved(env, 'GATEWAY_POSTGRES_PASSWORD');
  }

  const hosts = expectedHosts(environment);
  const allowedHosts = csv(env.GATEWAY_ALLOWED_HOSTS);
  for (const host of hosts) {
    if (!allowedHosts.includes(host)) {
      throw new Error(`GATEWAY_ALLOWED_HOSTS is missing ${host}`);
    }
  }
  const corsHosts = csv(env.GATEWAY_CORS_ALLOWED_ORIGINS).map(
    (origin) => new URL(origin).hostname,
  );
  for (const host of hosts) {
    if (!corsHosts.includes(host)) {
      throw new Error(`GATEWAY_CORS_ALLOWED_ORIGINS is missing ${host}`);
    }
  }

  if (mode === 'external') {
    requireResolved(env, 'GATEWAY_POSTGRES_HOST');
    requireResolved(env, 'GATEWAY_CLOUDROUTER_REDIS_HOST');
  }
  if (environment === 'production') {
    for (const key of PRODUCTION_SECRET_KEYS) {
      requireResolved(env, key);
    }
    if (String(env.GATEWAY_IM_ALLOW_ALL_PRINCIPALS).toLowerCase() !== 'false') {
      throw new Error('production must set GATEWAY_IM_ALLOW_ALL_PRINCIPALS=false');
    }
  }

  return { environment, expectedProfileId, hosts, mode };
}

function runComposeConfig(appRoot, envFile, mode) {
  const args = ['compose', '--env-file', envFile, '-f', 'docker-compose.yml'];
  if (mode === 'external') {
    args.push('-f', 'docker-compose.external.yml');
  }
  args.push('config', '--quiet');
  const result = spawnSync('docker', args, {
    cwd: appRoot,
    encoding: 'utf8',
    shell: false,
  });
  if (result.error) {
    throw new Error(`docker compose is unavailable: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`docker compose config failed: ${result.stderr || result.stdout}`);
  }
}

function main() {
  const { values } = parseArgs({
    args: process.argv.slice(2),
    options: {
      compose: { type: 'boolean', default: false },
      'env-file': { type: 'string' },
      mode: { type: 'string', default: 'embedded' },
    },
    strict: true,
  });
  if (!values['env-file']) {
    throw new Error('--env-file is required');
  }
  const scriptPath = fileURLToPath(import.meta.url);
  const appRoot = path.resolve(path.dirname(scriptPath), '..');
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

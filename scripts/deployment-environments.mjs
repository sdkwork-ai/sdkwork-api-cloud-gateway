#!/usr/bin/env node
/**
 * Canonical deployment-environment registry for sdkwork-api-cloud-gateway.
 *
 * Single source of truth for the lifecycle environment set, the per-environment
 * PostgreSQL identities, and the deterministic host-port plan. Every packager,
 * validator, deploy script, and contract test imports from here instead of
 * re-declaring the list (DEPLOYMENT_SPEC §5/§6, ENVIRONMENT_SPEC §2).
 *
 * CLI (for shell scripts; keep shell lists out of source):
 *   node scripts/deployment-environments.mjs list
 *   node scripts/deployment-environments.mjs check <environment>
 *   node scripts/deployment-environments.mjs port <environment>
 */

import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

export const ENVIRONMENTS = Object.freeze([
  'development',
  'test',
  'staging',
  'demo',
  'production',
]);

// Workspace PostgreSQL database/schema/role identity per environment
// (ENVIRONMENT_SPEC §7: sdkwork_ai_dev/test/staging/demo/prod).
export const DATABASE_IDENTITIES = Object.freeze({
  development: 'sdkwork_ai_dev',
  test: 'sdkwork_ai_test',
  staging: 'sdkwork_ai_staging',
  demo: 'sdkwork_ai_demo',
  production: 'sdkwork_ai_prod',
});

// Public API host sub-domain suffix per environment; production has none
// (api.sdkwork.com, api-dev.sdkwork.com, ...).
export const ENVIRONMENT_SUFFIX = Object.freeze({
  development: 'dev',
  test: 'test',
  staging: 'staging',
  demo: 'demo',
  production: '',
});

// Deterministic per-environment host-port plan for the single-node compose
// (docker-compose.yml, GATEWAY_HOST_PORT). The unified install bundle derives
// instance ports from GATEWAY_HOST_PORT + (index - 1) * 10 (DEPLOYMENT_SPEC §6).
export const ENVIRONMENT_HOST_PORTS = Object.freeze({
  development: 3910,
  test: 3911,
  staging: 3912,
  demo: 3914,
  production: 3913,
});

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export function requireEnvironment(value) {
  const environment = String(value ?? '').trim();
  if (!ENVIRONMENTS.includes(environment)) {
    throw new Error(
      `unsupported environment: ${value} (expected one of ${ENVIRONMENTS.join(', ')})`,
    );
  }
  return environment;
}

export function environmentHostPort(environment) {
  return ENVIRONMENT_HOST_PORTS[requireEnvironment(environment)];
}

function main(argv) {
  const [command, argument] = argv;
  switch (command) {
    case 'list':
      console.log(ENVIRONMENTS.join(' '));
      return 0;
    case 'check':
      requireEnvironment(argument);
      console.log(argument);
      return 0;
    case 'port':
      console.log(environmentHostPort(argument));
      return 0;
    default:
      console.error(
        'usage: node scripts/deployment-environments.mjs <list|check <environment>|port <environment>>',
      );
      return 2;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 2;
  }
}

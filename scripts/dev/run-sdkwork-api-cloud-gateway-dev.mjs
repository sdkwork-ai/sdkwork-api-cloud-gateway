#!/usr/bin/env node

import { spawn } from 'node:child_process';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { executePostgresInitWithNode } from '../../../sdkwork-specs/tools/postgres/postgres-init-node.mjs';
import { resolvePostgresDevProfile } from '../../../sdkwork-specs/tools/postgres/postgres-dev-profile.mjs';

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const collapsedFeatures = [
  'manager-operations-suite',
  'foundation-kernel',
  'foundation-deployments',
  'foundation-dezhou',
  'foundation-documents',
  'foundation-modelkit',
  'foundation-prompts',
  'foundation-web-server',
];
const embeddedDatabaseServices = [
  'DRIVE',
  'ORDER',
  'PROMOTION',
  'PAYMENT',
  'IAM',
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

const profile = resolvePostgresDevProfile({
  repoRoot: appRoot,
  extraEnv: {
    SDKWORK_APP_ROOT: appRoot,
    SDKWORK_DEPLOYMENT_ENV: 'development',
    SDKWORK_DEPLOYMENT_PROFILE: 'standalone',
    SDKWORK_RUNTIME_TARGET: 'server',
    SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC: 'false',
    SDKWORK_AGENT_RUNTIME_DATABASE_ENGINE:
      process.env.SDKWORK_AGENT_RUNTIME_DATABASE_ENGINE ?? 'postgres',
    SDKWORK_WEB_SNOWFLAKE_NODE_ID: process.env.SDKWORK_WEB_SNOWFLAKE_NODE_ID ?? '0',
  },
});

await executePostgresInitWithNode(profile.config);
process.stdout.write(
  `[sdkwork-api-cloud-gateway] PostgreSQL ready: ${profile.config.database.database}/${profile.config.database.schema}\n`,
);

const embeddedDatabaseUrl = new URL(profile.databaseUrl);
embeddedDatabaseUrl.searchParams.set(
  'options',
  `-c search_path=${profile.config.database.schema}`,
);
const childEnv = {
  ...profile.env,
  SDKWORK_CLAW_DATABASE_URL: embeddedDatabaseUrl.toString(),
  SDKWORK_DATABASE_URL: embeddedDatabaseUrl.toString(),
  PGOPTIONS: `-c search_path=${profile.config.database.schema}`,
};
for (const service of embeddedDatabaseServices) {
  childEnv[`SDKWORK_${service}_DATABASE_URL`] = embeddedDatabaseUrl.toString();
  childEnv[`SDKWORK_${service}_DATABASE_SCHEMA`] = profile.config.database.schema;
}

const child = spawn(
  'cargo',
  [
    'run',
    '-p',
    'sdkwork-api-cloud-gateway',
    '--bin',
    'sdkwork-api-cloud-gateway',
    '--features',
    collapsedFeatures.join(','),
    '--',
    '--config',
    'configs/sdkwork-api-cloud-gateway.development.toml.example',
  ],
  {
    cwd: appRoot,
    env: childEnv,
    shell: false,
    stdio: 'inherit',
  },
);

child.on('error', (error) => {
  process.stderr.write(`[sdkwork-api-cloud-gateway] failed to start cargo: ${error.message}\n`);
  process.exitCode = 1;
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exitCode = code ?? 1;
});

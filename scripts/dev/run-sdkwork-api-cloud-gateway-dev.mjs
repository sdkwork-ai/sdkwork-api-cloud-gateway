#!/usr/bin/env node

import { execFileSync, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

import {
  ensurePrivateRuntimeStateDirectory,
  resolveRepositoryRuntimeStateDirectory,
} from '../../../sdkwork-app-topology/tools/topology/lib/index.mjs';
import { executePostgresInitWithNode } from '../../../sdkwork-specs/tools/postgres/postgres-init-node.mjs';
import {
  isPostgresDevProfile,
  resolvePostgresDevProfile,
} from '../../../sdkwork-specs/tools/postgres/postgres-dev-profile.mjs';
import {
  resolveEmbeddedCanvasSqliteUrl,
  resolveEmbeddedDatabaseEnv,
  resolveEmbeddedDependencyEnv,
  resolveEmbeddedPostgresUrl,
  resolveTemporaryDatabaseDriverEnv,
} from './sdkwork-api-cloud-gateway-dev-env.mjs';
import {
  assertRemoteCloudDevelopmentProfile,
  resolveGatewaySourceProfile,
} from './sdkwork-api-cloud-gateway-source-config.mjs';
import {
  resolveEmbeddedKnowledgebaseRpcPlan,
  startEmbeddedKnowledgebaseRpc,
  stopEmbeddedKnowledgebaseRpc,
} from './embedded-knowledgebase-rpc.mjs';

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const collapsedFeatures = [
  'platform-foundation-suite',
];
const requiredBuildCriticalSources = [
  'Cargo.toml',
  'crates/sdkwork-api-cloud-gateway/Cargo.toml',
  'crates/sdkwork-api-cloud-gateway/src/listener_main.rs',
  'etc/sdkwork.deployment.config.json',
  'etc/sdkwork-api-cloud-gateway.development.toml',
  'scripts/dev/sdkwork-api-cloud-gateway-dev-env.mjs',
  'scripts/dev/sdkwork-api-cloud-gateway-source-config.mjs',
  'scripts/dev/embedded-knowledgebase-rpc.mjs',
];

async function main() {
  const selected = resolveSelectedProfile();
  ensureBuildCriticalSources();
  const sourceProfile = resolveGatewaySourceProfile({ appRoot, ...selected });

  if (selected.deploymentProfile === 'cloud') {
    assertRemoteCloudDevelopmentProfile(sourceProfile);
    process.stdout.write(
      `[sdkwork-api-cloud-gateway] resolved profileId=${sourceProfile.profileId} `
      + `runtimeTarget=${sourceProfile.env.SDKWORK_API_CLOUD_GATEWAY_RUNTIME_TARGET} `
      + `remoteGateway=${sourceProfile.env.SDKWORK_API_CLOUD_GATEWAY_PUBLIC_HTTP_URL}; `
      + 'no local gateway or database process started\n',
    );
    return;
  }

  if (selected.database !== 'postgres') {
    throw new Error('standalone development requires --database postgres');
  }
  if (!sourceProfile.runtimeConfig) {
    throw new Error(`${sourceProfile.profileId} must select a gateway runtime config`);
  }

  const profile = resolvePostgresDevProfile({
    repoRoot: appRoot,
    env: { ...sourceProfile.env, ...process.env },
    extraEnv: {
      SDKWORK_APP_ROOT: appRoot,
      SDKWORK_DEPLOYMENT_ENV: selected.environment,
      SDKWORK_DEPLOYMENT_PROFILE: selected.deploymentProfile,
      SDKWORK_RUNTIME_TARGET: 'server',
      SDKWORK_ENVIRONMENT: selected.environment,
      SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT: selected.environment,
      SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID: sourceProfile.profileId,
      SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC: 'false',
      SDKWORK_DATABASE_ENGINE:
        process.env.SDKWORK_DATABASE_ENGINE ?? 'postgresql',
    },
  });
  if (!isPostgresDevProfile(profile.env)) {
    throw new Error(`${sourceProfile.profileId} must resolve the PostgreSQL development profile`);
  }

  await executePostgresInitWithNode(profile.config);
  process.stdout.write(
    `[sdkwork-api-cloud-gateway] resolved profileId=${sourceProfile.profileId} `
    + `runtimeTarget=server databaseProfile=postgres config=${sourceProfile.runtimeConfig}\n`,
  );
  process.stdout.write(
    `[sdkwork-api-cloud-gateway] PostgreSQL ready: ${profile.config.database.database}/${profile.config.database.schema}\n`,
  );

  const childEnv = resolveChildEnv(profile);
  const knowledgebaseRpcPlan = resolveEmbeddedKnowledgebaseRpcPlan({
    databaseSchema: profile.config.database.schema,
    databaseUrl: profile.databaseUrl,
    env: childEnv,
    gatewayRoot: appRoot,
    pgOptions: childEnv.PGOPTIONS,
    runtimeStateRoot: resolveRepositoryRuntimeStateDirectory({
      owner: 'sdkwork-api-cloud-gateway',
      repoRoot: appRoot,
    }),
  });
  const runtimeConfig = process.env.SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE
    ?? sourceProfile.runtimeConfig;
  const knowledgebaseRpc = await startEmbeddedKnowledgebaseRpc(knowledgebaseRpcPlan);
  try {
    await runGateway(runtimeConfig, {
      ...childEnv,
      ...knowledgebaseRpcPlan?.clientEnv,
    });
  } finally {
    await stopEmbeddedKnowledgebaseRpc(knowledgebaseRpc, knowledgebaseRpcPlan);
  }
}

function resolveSelectedProfile() {
  const { values } = parseArgs({
    args: process.argv.slice(2),
    options: {
      database: { type: 'string' },
      'deployment-profile': { type: 'string' },
      environment: { type: 'string' },
    },
    strict: true,
  });
  return {
    database: values.database,
    deploymentProfile: values['deployment-profile'] ?? 'standalone',
    environment: values.environment ?? 'development',
  };
}

function ensureBuildCriticalSources() {
  const missing = requiredBuildCriticalSources.filter(
    (relativePath) => !existsSync(path.join(appRoot, relativePath)),
  );
  if (missing.length === 0) {
    return;
  }
  try {
    execFileSync('git', ['checkout', 'HEAD', '--', ...missing], {
      cwd: appRoot,
      stdio: 'ignore',
    });
  } catch {
    // The error below names every source that could not be recovered from git.
  }
  const unrecovered = missing.filter(
    (relativePath) => !existsSync(path.join(appRoot, relativePath)),
  );
  if (unrecovered.length > 0) {
    throw new Error(
      `Missing build-critical source files: ${unrecovered.join(', ')}. `
      + `Recover with: git checkout HEAD -- ${unrecovered.join(' ')}`,
    );
  }
}

function resolveChildEnv(profile) {
  const embeddedDatabaseUrl = resolveEmbeddedPostgresUrl(
    profile.databaseUrl,
    profile.config.database.schema,
  );
  const runtimeStateDirectory = ensurePrivateRuntimeStateDirectory({
    owner: 'sdkwork-api-cloud-gateway',
    repoRoot: appRoot,
  });
  const childEnv = {
    ...profile.env,
    ...resolveTemporaryDatabaseDriverEnv(profile.env),
    ...resolveEmbeddedDependencyEnv(profile.env),
    ...resolveEmbeddedDatabaseEnv({
      databaseUrl: embeddedDatabaseUrl,
      env: profile.env,
      schema: profile.config.database.schema,
    }),
    // Canvas pages use the client-local SQLite store (ENVIRONMENT_SPEC §7.2),
    // which must be resolvable even though every other module runs on the
    // PostgreSQL server profile.
    SDKWORK_DATABASE_SQLITE_URL: resolveEmbeddedCanvasSqliteUrl({
      env: profile.env,
      runtimeStateDirectory,
    }),
    PGOPTIONS: `-csearch_path=${profile.config.database.schema}`,
  };
  return childEnv;
}

function runGateway(runtimeConfig, childEnv) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      'cargo',
      [
        'run',
        '-p',
        'sdkwork-api-cloud-gateway',
        '--bin',
        'sdkwork-api-cloud-gateway',
        '--no-default-features',
        '--features',
        collapsedFeatures.join(','),
        '--',
        '--config',
        runtimeConfig,
      ],
      {
        cwd: appRoot,
        env: childEnv,
        shell: false,
        stdio: 'inherit',
      },
    );

    const forwardSignal = (signal) => {
      if (child.exitCode === null && child.signalCode === null) {
        child.kill(signal);
      }
    };
    const onSigint = () => forwardSignal('SIGINT');
    const onSigterm = () => forwardSignal('SIGTERM');
    const removeSignalHandlers = () => {
      process.off('SIGINT', onSigint);
      process.off('SIGTERM', onSigterm);
    };
    process.once('SIGINT', onSigint);
    process.once('SIGTERM', onSigterm);

    child.once('error', (error) => {
      removeSignalHandlers();
      reject(new Error(`failed to start cargo: ${error.message}`, { cause: error }));
    });
    child.once('exit', (code, signal) => {
      removeSignalHandlers();
      if (signal) {
        reject(new Error(`cargo exited from signal ${signal}`));
        return;
      }
      if (code !== 0) {
        reject(new Error(`cargo exited with code ${code ?? 'unknown'}`));
        return;
      }
      resolve();
    });
  });
}

main().catch((error) => {
  process.stderr.write(
    `[sdkwork-api-cloud-gateway] ${error instanceof Error ? error.message : String(error)}\n`,
  );
  process.exitCode = 1;
});

#!/usr/bin/env node
/**
 * Package per-environment docker deployment bundles for
 * sdkwork-api-cloud-gateway (DEPLOYMENT_SPEC §6, PNPM_SCRIPT_SPEC §4.4).
 *
 * Output layout per environment (dist/docker-packages/<environment>/):
 *   docker-compose.yml / docker-compose.embedded.yml   repo compose (paths intact)
 *   docker/config/config.toml                          host-mounted runtime config
 *   docker/postgres/                                   embedded-mode assets
 *   docker/env/<environment>.env                       operator-editable env file
 *   README.md / manifest.json
 *
 * Design contract:
 *   - Environment list, database identities, and host ports come from the
 *     shared registry (scripts/deployment-environments.mjs) — never re-declared.
 *   - Bundles are built from `docker/env/<environment>.env.example` only:
 *     operator secrets (git-ignored docker/env/<environment>.env) are NEVER
 *     copied into a deliverable (SUPPLY_CHAIN_SECURITY_SPEC).
 *   - The repo-relative compose layout is preserved so every bind mount
 *     (./docker/config/config.toml, ./docker/postgres/*) resolves inside the
 *     bundle without path rewriting.
 *   - Each environment file is validated with the same
 *     validateDeploymentEnvironment used by deploy-time checks before it is
 *     packaged.
 *
 * Public entry: `pnpm build:container:environments`; shell users go through
 * scripts/package-docker-environments.sh, which is a thin shim onto this file
 * (one implementation, one entry semantics).
 */

import { spawnSync } from 'node:child_process';
import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { parseDotEnv } from '../../../sdkwork-specs/tools/postgres/postgres-config.mjs';
import {
  DATABASE_IDENTITIES,
  ENVIRONMENTS,
  ENVIRONMENT_HOST_PORTS,
} from '../deployment-environments.mjs';
import {
  resolveExampleEnvironment,
  validateDeploymentEnvironment,
} from '../validate-docker-deployment.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const DEFAULT_OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'docker-packages');
const MANIFEST_SCHEMA_VERSION = '2026-09-02.environment-bundle.v1';

function parseArgs(argv) {
  const settings = { check: false, help: false, json: false, out: DEFAULT_OUTPUT_ROOT };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--') {
      continue;
    }
    if (argument === '--check') {
      settings.check = true;
    } else if (argument === '--json') {
      settings.json = true;
    } else if (argument === '--out') {
      settings.out = path.resolve(argv[++index]);
    } else if (argument === '--help' || argument === '-h') {
      settings.help = true;
    } else {
      throw new Error(`unsupported option: ${argument}`);
    }
  }
  return settings;
}

function printHelp() {
  console.log(`Usage: node scripts/docker/package-environment-bundles.mjs [options]

Package one self-contained docker deployment bundle per lifecycle environment
(development / test / staging / demo / production). Bundles are built from the
committed .env.example templates — operator secrets are never packaged.

Options:
  --check        Validate every environment template without writing output.
  --out <dir>    Output root (default: dist/docker-packages).
  --json         Print a machine-readable summary.
  -h, --help     Show this help.`);
}

function requireRepoFile(relativePath) {
  const absolutePath = path.join(REPO_ROOT, relativePath);
  if (!existsSync(absolutePath)) {
    throw new Error(`required repository file is missing: ${relativePath}`);
  }
  return absolutePath;
}

function validateEnvironmentTemplate(environment) {
  const examplePath = requireRepoFile(
    path.join('docker', 'env', `${environment}.env.example`),
  );
  const parsed = parseDotEnv(readFileSync(examplePath, 'utf8'));
  const summaries = [];
  for (const mode of ['external', 'embedded']) {
    summaries.push(
      validateDeploymentEnvironment(resolveExampleEnvironment(parsed, mode), mode),
    );
  }
  if (parsed.GATEWAY_HOST_PORT !== String(ENVIRONMENT_HOST_PORTS[environment])) {
    throw new Error(
      `${environment}.env.example GATEWAY_HOST_PORT must be ${ENVIRONMENT_HOST_PORTS[environment]}`,
    );
  }
  if (parsed.GATEWAY_POSTGRES_DB !== DATABASE_IDENTITIES[environment]) {
    throw new Error(
      `${environment}.env.example GATEWAY_POSTGRES_DB must be ${DATABASE_IDENTITIES[environment]}`,
    );
  }
  return summaries;
}

function packageEnvironment(environment, outputRoot) {
  const target = path.join(outputRoot, environment);
  rmSync(target, { recursive: true, force: true });
  mkdirSync(path.join(target, 'docker', 'config'), { recursive: true });
  mkdirSync(path.join(target, 'docker', 'env'), { recursive: true });

  // Repo-relative layout is preserved so docker-compose.yml bind mounts
  // (./docker/config/config.toml, ./docker/postgres/*) resolve as-is.
  copyFileSync(requireRepoFile('docker-compose.yml'), path.join(target, 'docker-compose.yml'));
  copyFileSync(
    requireRepoFile('docker-compose.embedded.yml'),
    path.join(target, 'docker-compose.embedded.yml'),
  );
  copyFileSync(
    requireRepoFile(path.join('docker', 'config', 'config.toml')),
    path.join(target, 'docker', 'config', 'config.toml'),
  );
  cpSync(
    requireRepoFile(path.join('docker', 'postgres')),
    path.join(target, 'docker', 'postgres'),
    { recursive: true },
  );
  // From the committed example template only — never the operator's local
  // secret file. The example becomes the operator-editable .env in the bundle.
  copyFileSync(
    requireRepoFile(path.join('docker', 'env', `${environment}.env.example`)),
    path.join(target, 'docker', 'env', `${environment}.env`),
  );

  const port = ENVIRONMENT_HOST_PORTS[environment];
  writeFileSync(
    path.join(target, 'README.md'),
    `# SDKWork API Cloud Gateway — ${environment} docker deployment bundle

- Environment: ${environment} (profile \`standalone.${environment}\`)
- Host port: ${port} (host mapping ${port} -> 3900 in the container); embedded
  mode additionally requires the optional in-compose databases below.
- Config mount: \`docker/config/config.toml\` ->
  \`/etc/sdkwork/api-gateway/config.toml\` (same path as the Ubuntu install;
  APPLICATION_DEPLOY_LAYOUT_SPEC). Environment variables from
  \`docker/env/${environment}.env\` win over this file (ENVIRONMENT_SPEC §3).
- Dependencies: external PostgreSQL + Redis by default — edit
  \`docker/env/${environment}.env\` (GATEWAY_POSTGRES_* /
  GATEWAY_CLOUDROUTER_REDIS_*) before exposing anything beyond localhost.
  In-compose PostgreSQL + Redis is opt-in via \`docker-compose.embedded.yml\`.
- Reverse proxy: configure the independent \`sdkwork-webserver\` deployment to
  upstream \`127.0.0.1:${port}\` — this bundle installs no nginx.

## Fill secrets

    # Edit the operator env file copied from the committed example
    $EDITOR docker/env/${environment}.env

## Deploy (from this bundle directory)

    docker compose --env-file docker/env/${environment}.env \\
      -p sdkwork-gateway-${environment} -f docker-compose.yml up -d

    # Built-in PostgreSQL + Redis instead (disposable smoke tests):
    docker compose --env-file docker/env/${environment}.env --profile embedded \\
      -p sdkwork-gateway-${environment} -f docker-compose.yml \\
      -f docker-compose.embedded.yml up -d

## Verify

    curl -fsS http://127.0.0.1:${port}/healthz
`,
    'utf8',
  );
  writeFileSync(
    path.join(target, 'manifest.json'),
    `${JSON.stringify(
      {
        schemaVersion: MANIFEST_SCHEMA_VERSION,
        kind: 'sdkwork.environment-deployment-bundle',
        application: 'sdkwork-api-cloud-gateway',
        environment,
        profileId: `standalone.${environment}`,
        hostPort: port,
        databaseIdentity: DATABASE_IDENTITIES[environment],
        dependencyModes: ['external', 'embedded'],
        configMount: '/etc/sdkwork/api-gateway/config.toml',
        healthPath: '/healthz',
        readinessPath: '/readyz',
        spec: ['DEPLOYMENT_SPEC.md#6', 'PNPM_SCRIPT_SPEC.md#4.4'],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  return { environment, target, hostPort: port };
}

function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    printHelp();
    return 0;
  }

  const validation = ENVIRONMENTS.map((environment) => ({
    environment,
    summaries: validateEnvironmentTemplate(environment),
  }));

  if (settings.check) {
    if (settings.json) {
      console.log(JSON.stringify({ ok: true, environments: ENVIRONMENTS }, null, 2));
    } else {
      console.log(`[environment-bundles] validated ${ENVIRONMENTS.length} environment templates`);
    }
    return 0;
  }

  const packaged = validation.map(({ environment }) => packageEnvironment(environment, settings.out));
  if (settings.json) {
    console.log(JSON.stringify({ ok: true, outputRoot: settings.out, packages: packaged }, null, 2));
  } else {
    for (const entry of packaged) {
      console.log(`[environment-bundles] packaged ${entry.environment} -> ${entry.target}`);
    }
    console.log(`[environment-bundles] done: ${settings.out}`);
  }
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = main();
  } catch (error) {
    console.error(`[environment-bundles] ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}

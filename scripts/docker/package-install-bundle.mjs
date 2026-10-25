#!/usr/bin/env node
// Package the unified sdkwork-api-cloud-gateway Docker install bundle
// (PNPM_SCRIPT_SPEC.md §4.4 / DEPLOYMENT_SPEC.md §6):
//
//   dist/docker-install/sdkwork-api-cloud-gateway-install-<version>.bundle/
//     image.tar.gz / image.sha256 / image.env
//     compose/docker-compose.bundle.yml       environment-neutral multi-instance
//     env/{development,test,staging,production}.env.example
//     deploy.sh / manifest.json / README.md
//
// The image is built once (delegating to
// scripts/build-api-cloud-gateway-container.mjs) and carries no environment
// binding; environments, replica counts, and all deployment inputs are
// resolved by the bundle deploy.sh at container start.
//
// Public script: `pnpm build:container:install`.

import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { pipeline } from 'node:stream/promises';
import { createReadStream, createWriteStream } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createGzip } from 'node:zlib';

import { ENVIRONMENTS } from '../deployment-environments.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const DEFAULT_OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'docker-install');
const DEFAULT_IMAGE_REPOSITORY = 'registry.sdkwork.com/apps/sdkwork-api-cloud-gateway';

function appVersion() {
  const manifest = JSON.parse(
    readFileSync(path.join(REPO_ROOT, 'agent.app.config.json'), 'utf8'),
  );
  const version = manifest?.release?.currentVersion;
  if (typeof version !== 'string' || version.length === 0) {
    throw new Error('agent.app.config.json release.currentVersion is missing');
  }
  return version;
}

// The version composes the bundle directory name and the image ref; mirror
// package-server.mjs's normalizeVersion allowlist so neither can escape the
// output root or fragment a docker ref.
function normalizeVersion(value) {
  const text = String(value ?? '').trim();
  if (!/^[0-9A-Za-z][0-9A-Za-z._+-]*$/u.test(text)) {
    throw new Error(`Invalid package version: ${value}`);
  }
  return text;
}

function parseArgs(argv) {
  const settings = {
    version: normalizeVersion(appVersion()),
    tag: undefined,
    repository: process.env.SDKWORK_CONTAINER_REPOSITORY || DEFAULT_IMAGE_REPOSITORY,
    outputRoot: DEFAULT_OUTPUT_ROOT,
    skipImageBuild: false,
    dryRun: false,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    // `pnpm run <script> -- --flag` forwards a literal `--` on some pnpm
    // versions/platforms; ignore it.
    if (argument === '--') {
      continue;
    }
    if (argument === '--version') {
      settings.version = normalizeVersion(argv[++index]);
    } else if (argument === '--tag') {
      settings.tag = argv[++index];
    } else if (argument === '--repository') {
      settings.repository = argv[++index];
    } else if (argument === '--out') {
      settings.outputRoot = path.resolve(argv[++index]);
    } else if (argument === '--skip-image-build') {
      settings.skipImageBuild = true;
    } else if (argument === '--dry-run') {
      settings.dryRun = true;
    } else if (argument === '--help' || argument === '-h') {
      settings.help = true;
    } else {
      throw new Error(`unsupported option: ${argument}`);
    }
  }
  settings.tag ??= settings.version;
  settings.tag = normalizeVersion(settings.tag);
  return settings;
}

function printHelp() {
  console.log(`Usage: node scripts/docker/package-install-bundle.mjs [options]

Build the unified install image and package the self-contained install bundle.

Options:
  --version <semver>   Default: agent.app.config.json release.currentVersion
  --tag <image-tag>    Default: release version
  --repository <repo>  Image repository (default: ${DEFAULT_IMAGE_REPOSITORY})
  --out <dir>          Output root (default: dist/docker-install)
  --skip-image-build   Require an already-built image; do not invoke the container build
  --dry-run            Print the resolved plan only
  --help, -h           Show this help`);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? REPO_ROOT,
    encoding: 'utf8',
    stdio: options.stdio ?? 'inherit',
    timeout: options.timeoutMs ?? 60 * 60 * 1000,
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    const detail = result.error?.message ?? `exit ${result.status}`;
    throw new Error(`${command} ${args.join(' ')} failed: ${detail}`);
  }
  return result;
}

function imageRef(repository, tag) {
  return `${repository}:${tag}`;
}

function imageExists(ref) {
  return spawnSync('docker', ['image', 'inspect', ref], {
    stdio: 'ignore',
    windowsHide: true,
  }).status === 0;
}

function buildImage(settings, ref) {
  if (imageExists(ref)) {
    console.log(`image already present: ${ref}`);
    return;
  }
  if (settings.skipImageBuild) {
    throw new Error(`image missing and --skip-image-build set: ${ref}`);
  }
  // Delegate to the committed container build (release binary + database
  // modules + docker build). The local parity tag keeps the existing
  // `pnpm build:container` workflow usable alongside the bundle tag.
  run('node', [
    'scripts/build-api-cloud-gateway-container.mjs',
    '--version',
    settings.version,
    '--tag',
    ref,
  ]);
  if (!imageExists(ref)) {
    throw new Error(`image was not produced: ${ref}`);
  }
}

async function saveImage(ref, destination) {
  // Stream docker save through gzip (cross-platform; no shell pipeline).
  const child = spawn('docker', ['save', ref], {
    stdio: ['ignore', 'pipe', 'inherit'],
    windowsHide: true,
  });
  // Attach the exit listener synchronously after spawn: if the child exits
  // while pipeline() drains stdout, a late-attached listener would miss the
  // 'exit' event, leave this promise pending forever, and make node exit 0
  // silently with the bundle half-populated.
  const exited = new Promise((resolve, reject) => {
    child.on('exit', (code) => resolve(code));
    child.on('error', reject);
  });
  await pipeline(child.stdout, createGzip({ level: 1 }), createWriteStream(destination));
  const code = await exited;
  if (code !== 0) {
    throw new Error(`docker save exited ${code}`);
  }
}

async function sha256File(filePath) {
  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    const stream = createReadStream(filePath);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.on('error', reject);
    stream.on('end', () => resolve(hash.digest('hex')));
  });
}

const BUNDLE_README = `# sdkwork-api-cloud-gateway install bundle

Self-contained deployment bundle for the unified install image
(DEPLOYMENT_SPEC.md §6 / PNPM_SCRIPT_SPEC.md §4.4). One image, any
environment (development / test / staging / demo / production), N gateway
instances per environment.

Two dependency modes (one image for both):

  external (DEFAULT): point GATEWAY_POSTGRES_* / GATEWAY_CLOUDROUTER_REDIS_*
    in env/<environment>.env at your managed PostgreSQL + Redis instances.

  embedded (opt-in): the deps project starts its own PostgreSQL (pgvector)
    + Redis. Add --embedded; no managed databases required.

Quick start on any Docker host:

    # 1. Prepare configuration (only keys you care about; defaults work for
    #    a first smoke test). prepare-envs.sh creates EVERY environment's
    #    env file at once with distinct snowflake node ids for co-located
    #    fleets; a plain cp works for a single environment too.
    bash prepare-envs.sh
    #    Local verification only: also fill the production Cloud Router
    #    secrets from the dev defaults (never for real production):
    # bash prepare-envs.sh --dev-secrets-for-production

    # 2. Deploy — external PostgreSQL + Redis (default)
    bash deploy.sh --environment development

    # 2b. Deploy — built-in PostgreSQL + Redis instead (no external setup)
    bash deploy.sh --environment development --embedded

    # Production with 3 instances
    bash deploy.sh --environment production --replicas 3

    # Configuration check only (fail-closed env preflight + live external
    # PostgreSQL/Redis probes; deploys nothing)
    bash deploy.sh --environment production --check-config

    # Write one missing value into the env file (never rewrites an
    # already-configured value; --force-set replaces deliberately)
    bash deploy.sh --environment production --set GATEWAY_POSTGRES_PASSWORD '<value>'

    # Day-2 operations
    bash deploy.sh --environment test --ps
    bash deploy.sh --environment test --logs 2
    bash deploy.sh --environment test --restart
    bash deploy.sh --environment test --stop
    bash deploy.sh --environment test --start
    bash deploy.sh --environment test --down
    bash deploy.sh --environment test --down --purge

Release lifecycle (deploy with health gate + auto rollback + version ledger):

    bash release.sh deploy   --environment test                  # upgrade in place
    bash release.sh deploy   --environment test --image-tag 0.1.2
    bash release.sh status   --environment test                  # version + health + drift
    bash release.sh rollback --environment test                  # one step back
    bash release.sh rollback --environment test --to 0.1.0
    bash release.sh history  --environment test                  # release ledger
    bash release.sh verify   --environment test                  # health probe only

1. deploy.sh creates env/<environment>.env from the example when missing —
   fill secrets (PostgreSQL, Redis, Cloud Router keys) before exposing
   anything beyond localhost.
2. deploy.sh loads image.tar.gz when the image is missing, starts the
   per-environment knowledgebase-rpc singleton, then starts gateway
   instances 1..N. Instance 1 owns the database migration; instances 2..N
   start only after instance 1 is healthy.
3. Instance host ports: GATEWAY_HOST_PORT + (index - 1) * 10 -> container
   3900 (e.g. production base 3913: 3913, 3923, 3933, ...). Load-balance
   across them from sdkwork-webserver upstreams.
4. Health: curl http://127.0.0.1:<instance-port>/healthz (readiness:
   /readyz).

Env file key contract (env/<environment>.env):
  - Host-side keys use the GATEWAY_* prefix; docker compose maps them to
    the container-side SDKWORK_* names (compose only interpolates what it
    declares, so a key the compose file never references is dead weight).
  - The ONLY SDKWORK_* keys the compose templates consume from the env
    file are the database pool tunables SDKWORK_DATABASE_AUTO_MIGRATE,
    SDKWORK_DATABASE_MAX_CONNECTIONS, SDKWORK_DATABASE_MIN_CONNECTIONS,
    SDKWORK_DATABASE_ACQUIRE_TIMEOUT, SDKWORK_DATABASE_IDLE_TIMEOUT,
    SDKWORK_DATABASE_MAX_LIFETIME, SDKWORK_DATABASE_TEMPORARY_* plus the
    optional SDKWORK_DATABASE_URL credential override. Keep exactly these;
    other SDKWORK_* lines in the env file are inert (container env comes
    from the image ENV + the compose environment section).
  - deploy.sh --check-config verifies every required key fail-closed and,
    in external mode, live-probes PostgreSQL auth and Redis ping before
    anything is deployed.

Full guide: docs/guides/operator/docker-install.md (中文) /
docker-install.en.md (English).
`;

async function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    printHelp();
    return;
  }
  const ref = imageRef(settings.repository, settings.tag);
  const bundleDir = path.join(
    settings.outputRoot,
    `sdkwork-api-cloud-gateway-install-${settings.version}.bundle`,
  );
  console.log(`bundle directory: ${bundleDir}`);
  console.log(`image: ${ref}`);
  if (settings.dryRun) {
    console.log(`docker save ${ref} | gzip -> ${path.join(bundleDir, 'image.tar.gz')}`);
    console.log(`compose templates: docker-compose.bundle.yml + docker-compose.bundle.embedded.yml`);
    console.log(`embedded postgres assets: bin/container/postgres/entrypoint.sh + bin/container/postgres/init/`);
    console.log(`env templates: ${ENVIRONMENTS.map((env) => `docker/env/${env}.env.example`).join(', ')}`);
    console.log(`deploy script: bin/docker-bundle-deploy.sh -> deploy.sh`);
    console.log(`release lifecycle script: bin/docker-bundle-release.sh -> release.sh`);
    return;
  }

  buildImage(settings, ref);

  // Preserve operator-maintained env files across repacks. docker-install.md
  // §2.1 promises "repeated packaging never loses server configuration":
  // once prepare-envs.sh generated env/<environment>.env and the operator
  // filled it in, those files are the single live configuration source. The
  // bundle rebuild below replaces templates but MUST NOT delete live env
  // files (any env/*.env except the *.env.example templates, including
  // per-instance overrides like development.i1.env).
  const envDir = path.join(bundleDir, 'env');
  const preservedLiveEnvFiles = new Map();
  if (existsSync(envDir)) {
    for (const entry of readdirSync(envDir)) {
      if (entry.endsWith('.env') && !entry.endsWith('.env.example')) {
        preservedLiveEnvFiles.set(entry, readFileSync(path.join(envDir, entry)));
      }
    }
  }

  rmSync(bundleDir, { recursive: true, force: true });
  mkdirSync(path.join(bundleDir, 'compose'), { recursive: true });
  mkdirSync(path.join(bundleDir, 'env'), { recursive: true });
  mkdirSync(path.join(bundleDir, 'postgres'), { recursive: true });

  console.log(`saving image ${ref} (gzip, this can take several minutes)...`);
  const imageTgz = path.join(bundleDir, 'image.tar.gz');
  await saveImage(ref, imageTgz);
  const digest = await sha256File(imageTgz);
  writeFileSync(path.join(bundleDir, 'image.sha256'), `${digest}  image.tar.gz\n`);

  writeFileSync(
    path.join(bundleDir, 'image.env'),
    `SDKWORK_API_CLOUD_GATEWAY_IMAGE_TAG=${settings.tag}\n`,
  );

  copyFileSync(
    path.join(REPO_ROOT, 'docker-compose.bundle.yml'),
    path.join(bundleDir, 'compose', 'docker-compose.bundle.yml'),
  );
  copyFileSync(
    path.join(REPO_ROOT, 'docker-compose.bundle.embedded.yml'),
    path.join(bundleDir, 'compose', 'docker-compose.bundle.embedded.yml'),
  );
  // Attach-contract override (SDKWORK_WEBSERVER_SPEC §17.3): the webserver
  // public edge dials `sdkwork-api-cloud-gateway:8080`, so every instance
  // project must be composed with this override or module API proxying breaks.
  copyFileSync(
    path.join(REPO_ROOT, 'docker-compose.bundle.attach-override.yml'),
    path.join(bundleDir, 'compose', 'docker-compose.bundle.attach-override.yml'),
  );
  // Embedded-mode PostgreSQL assets: TLS-generating entrypoint + schema init
  // (deploy.sh exports POSTGRES_ASSETS_DIR at deploy time).
  copyFileSync(
    path.join(REPO_ROOT, 'bin', 'container', 'postgres', 'entrypoint.sh'),
    path.join(bundleDir, 'postgres', 'entrypoint.sh'),
  );
  cpSync(
    path.join(REPO_ROOT, 'bin', 'container', 'postgres', 'init'),
    path.join(bundleDir, 'postgres', 'init'),
    { recursive: true },
  );
  for (const environment of ENVIRONMENTS) {
    const source = path.join(REPO_ROOT, 'docker', 'env', `${environment}.env.example`);
    if (!existsSync(source)) {
      throw new Error(`env template missing: ${source}`);
    }
    copyFileSync(source, path.join(bundleDir, 'env', `${environment}.env.example`));
  }
  for (const [name, content] of preservedLiveEnvFiles) {
    writeFileSync(path.join(bundleDir, 'env', name), content);
    console.log(`preserved operator env: env/${name}`);
  }
  // Source names are flat (MODULE_BIN_SPEC.md §2.2); the bundle root names are
  // fixed by DOCKER_SPEC.md §4.1 / APPLICATION_DEPLOY_LAYOUT_SPEC.md §9.1.
  const deployScript = path.join(bundleDir, 'deploy.sh');
  copyFileSync(path.join(REPO_ROOT, 'bin', 'docker-bundle-deploy.sh'), deployScript);
  const prepareEnvsScript = path.join(bundleDir, 'prepare-envs.sh');
  copyFileSync(
    path.join(REPO_ROOT, 'bin', 'docker-bundle-prepare-envs.sh'),
    prepareEnvsScript,
  );
  const releaseScript = path.join(bundleDir, 'release.sh');
  copyFileSync(path.join(REPO_ROOT, 'bin', 'docker-bundle-release.sh'), releaseScript);
  const chmod = spawnSync('chmod', ['0755', deployScript, prepareEnvsScript, releaseScript], { stdio: 'ignore' });
  if (chmod.status !== 0) {
    // Windows filesystems ignore the POSIX mode; harmless.
  }

  writeFileSync(
    path.join(bundleDir, 'manifest.json'),
    `${JSON.stringify(
      {
        kind: 'sdkwork.component-deployment-bundle',
        application: 'sdkwork-api-cloud-gateway',
        deploymentProfile: 'standalone',
        version: settings.version,
        image: ref,
        imageTag: settings.tag,
        imageSha256: digest,
        environments: ENVIRONMENTS,
        dependencyModes: ['external', 'embedded'],
        multiInstance: true,
        healthPath: '/healthz',
        createdAt: new Date().toISOString(),
        spec: ['DEPLOYMENT_SPEC.md#6', 'PNPM_SCRIPT_SPEC.md#4.4'],
      },
      null,
      2,
    )}\n`,
  );
  writeFileSync(path.join(bundleDir, 'README.md'), BUNDLE_README);

  console.log(`packaged ${bundleDir}`);
  console.log(`  image:    ${ref}`);
  console.log(`  sha256:   ${digest}`);
  console.log(`  deploy:   bash deploy.sh --environment <env> [--replicas N] [--embedded]`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});

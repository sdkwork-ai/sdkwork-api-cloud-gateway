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
  existsSync,
  mkdirSync,
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

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const DEFAULT_OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'docker-install');
const ENVIRONMENTS = Object.freeze(['development', 'test', 'staging', 'production']);
const DEFAULT_IMAGE_REPOSITORY = 'registry.sdkwork.com/apps/sdkwork-api-cloud-gateway';

function appVersion() {
  const manifest = JSON.parse(
    readFileSync(path.join(REPO_ROOT, 'sdkwork.app.config.json'), 'utf8'),
  );
  const version = manifest?.release?.currentVersion;
  if (typeof version !== 'string' || version.length === 0) {
    throw new Error('sdkwork.app.config.json release.currentVersion is missing');
  }
  return version;
}

function parseArgs(argv) {
  const settings = {
    version: appVersion(),
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
      settings.version = argv[++index];
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
  return settings;
}

function printHelp() {
  console.log(`Usage: node scripts/docker/package-install-bundle.mjs [options]

Build the unified install image and package the self-contained install bundle.

Options:
  --version <semver>   Default: sdkwork.app.config.json release.currentVersion
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
  await pipeline(child.stdout, createGzip({ level: 1 }), createWriteStream(destination));
  await new Promise((resolve, reject) => {
    child.on('exit', (code) => (code === 0 ? resolve() : reject(new Error(`docker save exited ${code}`))));
    child.on('error', reject);
  });
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
environment (development / test / staging / production), N gateway instances
per environment.

Quick start on any Docker host (external PostgreSQL + Redis required):

    bash deploy.sh --environment development
    bash deploy.sh --environment production --replicas 3
    bash deploy.sh --environment test --ps
    bash deploy.sh --environment test --down
    bash deploy.sh --environment test --down --purge

1. Copy env/<environment>.env.example to env/<environment>.env and fill
   secrets (PostgreSQL, Redis, Cloud Router keys) before exposing anything.
2. deploy.sh loads image.tar.gz when the image is missing, starts the
   per-environment knowledgebase-rpc singleton, then starts gateway
   instances 1..N. Instance 1 owns the database migration; instances 2..N
   start only after instance 1 is healthy.
3. Instance host ports: GATEWAY_HOST_PORT + (index - 1) * 10 -> container
   3900 (e.g. production base 3913: 3913, 3923, 3933, ...). Load-balance
   across them from sdkwork-webserver upstreams.

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
    console.log(`compose template: docker-compose.bundle.yml`);
    console.log(`env templates: ${ENVIRONMENTS.map((env) => `docker/env/${env}.env.example`).join(', ')}`);
    console.log(`deploy script: scripts/docker/bundle/deploy.sh`);
    return;
  }

  buildImage(settings, ref);

  rmSync(bundleDir, { recursive: true, force: true });
  mkdirSync(path.join(bundleDir, 'compose'), { recursive: true });
  mkdirSync(path.join(bundleDir, 'env'), { recursive: true });

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
  for (const environment of ENVIRONMENTS) {
    const source = path.join(REPO_ROOT, 'docker', 'env', `${environment}.env.example`);
    if (!existsSync(source)) {
      throw new Error(`env template missing: ${source}`);
    }
    copyFileSync(source, path.join(bundleDir, 'env', `${environment}.env.example`));
  }
  const deployScript = path.join(bundleDir, 'deploy.sh');
  copyFileSync(path.join(REPO_ROOT, 'scripts', 'docker', 'bundle', 'deploy.sh'), deployScript);
  const chmod = spawnSync('chmod', ['0755', deployScript], { stdio: 'ignore' });
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
  console.log(`  deploy:   bash deploy.sh --environment <env> [--replicas N]`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});

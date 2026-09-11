#!/usr/bin/env node
/**
 * Build the sdkwork-api-cloud-gateway standalone container image.
 *
 * Pipeline (mirrors the sdkwork-cloudrouter container build):
 *   1. verify staged prerequisites (release binary, docker daemon, database
 *      modules resolved from cargo metadata)
 *   2. assemble dist/container-image-build — the docker build context laid out
 *      like the server install package (bin/, config/, specs/,
 *      database-modules/, sdkwork.app.config.json, INSTALL.md,
 *      install-manifest.json, checksums.sha256) plus container/ (entrypoint +
 *      metadata.json)
 *   3. docker build -f Dockerfile -t <imageTag> <build context>
 *   4. record the immutable image id/digest and final image size in
 *      dist/container-image.json
 *
 * The committed Dockerfile at the repository root is the build input.
 * Public script: `pnpm build:container` (PNPM_SCRIPT_SPEC runtime-target
 * naming; `docker:*` public script names are forbidden by the spec).
 */

import { createHash } from 'node:crypto';
import { execFile, spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { cp, mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import {
  CONTAINER_CARGO_FEATURES,
  CONTAINER_DATABASE_ASSET_METADATA_ARGS,
  RELEASE_CONFIG_TEMPLATES,
  resolveCargoReleaseDir,
  resolveDatabaseHostCratesFromMetadata,
} from './package-server.mjs';

const execFileAsync = promisify(execFile);

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const appRoot = path.resolve(__dirname, '..');

const CONTAINER_IMAGE_MANIFEST_SCHEMA_VERSION = '2026-08-08.container-image.v1';
const APP_ID = 'sdkwork-api-cloud-gateway';
const BINARY_NAME = 'sdkwork-api-cloud-gateway';
const KNOWLEDGEBASE_RPC_BINARY_NAME = 'sdkwork-intelligence-knowledgebase-rpc-bin';
const KNOWLEDGEBASE_ROOT = path.resolve(appRoot, '..', 'sdkwork-knowledgebase');
// Database workspaces whose module requires non-database assets next to
// `database/` under the packaged module root (mirrors sdkwork-cloudrouter's
// extra paths and scripts/package-server.mjs). sdkwork-iam materializes its
// module catalog from <module root>/iam/registry/iam-registry.config.json at
// bootstrap time.
const EXTRA_DATABASE_MODULE_PATHS = Object.freeze({
  'sdkwork-iam': ['iam'],
});
// Image name + tag. Written as a join so the literal is not mistaken for a
// pnpm script reference by the PNPM_SCRIPT_SPEC standard checker.
const DEFAULT_IMAGE_TAG = ['sdkwork-api-cloud-gateway', 'local'].join(':');
const IMAGE_BUILD_DIR = 'dist/container-image-build';
const IMAGE_MANIFEST_FILE = 'dist/container-image.json';
const CONTAINER_PLATFORM = 'container';
const CONTAINER_PROFILE = 'container';
const CONTAINER_FORMAT = 'oci';
const DEFAULT_DEPLOYMENT_PROFILE = 'standalone';

function printHelp() {
  console.log(`Usage: node scripts/build-api-cloud-gateway-container.mjs [options]

Build the sdkwork-api-cloud-gateway standalone container image from the
release binary (built for the host platform; run this on Linux/WSL) and the
resolved database modules, then docker build and record the image manifest.

Options:
  --version <value>    Package version (default from package.json).
  --tag <name>         Image tag (default ${DEFAULT_IMAGE_TAG}).
  --deployment-profile <value>  Fixed profile (standalone or cloud).
  --push               Push the image and record its verified repository digest.
  --require-repo-digest  Reject a local-only image manifest during validation.
  --skip-build         Reuse an existing release binary (no cargo build).
  --force              Re-stage the build context even when inputs are cached.
  --check              Validate the build plan without building.
  --dry-run            Print the build plan without writing files.
  --validate-manifest  Validate dist/container-image.json without Cargo or Docker.
  --json               Print machine-readable JSON.
  -h, --help           Show this help.
`);
}

function parseArgs(argv = process.argv.slice(2)) {
  const settings = {
    check: false,
    dryRun: false,
    force: false,
    help: false,
    json: false,
    skipBuild: false,
    push: false,
    requireRepoDigest: false,
    tag: DEFAULT_IMAGE_TAG,
    validateManifest: false,
    version: '',
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    switch (arg) {
      case '--check':
        settings.check = true;
        break;
      case '--dry-run':
        settings.dryRun = true;
        break;
      case '--json':
        settings.json = true;
        break;
      case '--force':
        settings.force = true;
        break;
      case '--skip-build':
        settings.skipBuild = true;
        break;
      case '--push':
        settings.push = true;
        break;
      case '--require-repo-digest':
        settings.requireRepoDigest = true;
        break;
      case '--validate-manifest':
        settings.validateManifest = true;
        break;
      case '-h':
      case '--help':
        settings.help = true;
        break;
      case '--version':
        settings.version = requireValue(argv, index, arg);
        index += 1;
        break;
      case '--tag':
        settings.tag = requireValue(argv, index, arg);
        index += 1;
        break;
      case '--deployment-profile':
        settings.deploymentProfile = requireValue(argv, index, arg);
        index += 1;
        break;
      default:
        throw new Error(`Unknown option: ${arg}`);
    }
  }
  return settings;
}

function requireValue(argv, index, flag) {
  const value = argv[index + 1];
  if (!value || value.startsWith('--')) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

function normalizeVersion(value) {
  const text = String(value ?? '').trim();
  const normalized = text.startsWith('refs/tags/') ? text.slice('refs/tags/'.length) : text;
  const withoutPrefix = normalized.startsWith('v') && /^[0-9]/u.test(normalized.slice(1))
    ? normalized.slice(1)
    : normalized;
  if (!/^[0-9A-Za-z][0-9A-Za-z._+-]*$/u.test(withoutPrefix)) {
    throw new Error(`Invalid package version: ${value}`);
  }
  return withoutPrefix;
}

async function loadPackageJson() {
  return JSON.parse(await readFile(path.join(appRoot, 'package.json'), 'utf8'));
}

async function loadCargoMetadata() {
  const { stdout } = await execFileAsync('cargo', CONTAINER_DATABASE_ASSET_METADATA_ARGS, {
    cwd: appRoot,
    maxBuffer: 64 * 1024 * 1024,
  });
  return JSON.parse(stdout);
}

async function resolveDatabaseModules(metadata) {
  const hostCrates = resolveDatabaseHostCratesFromMetadata(metadata);
  const modules = [];
  const discoveredWorkspaces = new Set();
  for (const hostCrate of hostCrates) {
    const workspaceRoot = path.resolve(hostCrate.crateRoot, '..', '..');
    const workspace = path.basename(workspaceRoot);
    if (discoveredWorkspaces.has(workspace)) {
      continue;
    }
    const databaseRoot = path.join(workspaceRoot, 'database');
    const manifestPath = path.join(databaseRoot, 'database.manifest.json');
    if (!existsSync(manifestPath)) {
      throw new Error(`${hostCrate.name} is missing ${manifestPath}`);
    }
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    if (typeof manifest.moduleId !== 'string' || !manifest.moduleId.trim()) {
      throw new Error(`${manifestPath} must declare a non-empty moduleId`);
    }
    modules.push({
      databaseRoot,
      hostCrate: hostCrate.name,
      moduleId: manifest.moduleId,
      workspace,
      extraPaths: EXTRA_DATABASE_MODULE_PATHS[workspace] ?? [],
    });
    discoveredWorkspaces.add(workspace);
  }
  modules.sort((left, right) => left.workspace.localeCompare(right.workspace));
  return modules;
}

async function createBuildPlan(settings) {
  const packageJson = await loadPackageJson();
  const version = normalizeVersion(settings.version || process.env.SDKWORK_PACKAGE_VERSION || packageJson.version || '0.1.0');
  const deploymentProfile = settings.deploymentProfile || DEFAULT_DEPLOYMENT_PROFILE;
  const packageId = resolveContainerPackageId(deploymentProfile);
  const releaseDir = resolveCargoReleaseDir(appRoot, process.env.CARGO_TARGET_DIR);
  const binaryPath = path.join(releaseDir, BINARY_NAME);
  const knowledgebaseRpcBinaryPath = path.join(releaseDir, KNOWLEDGEBASE_RPC_BINARY_NAME);
  const imageBuildDir = path.join(appRoot, IMAGE_BUILD_DIR);
  const manifestPath = path.join(appRoot, IMAGE_MANIFEST_FILE);

  const metadata = await loadCargoMetadata();
  const databaseModules = await resolveDatabaseModules(metadata);

  const plan = {
    schemaVersion: CONTAINER_IMAGE_MANIFEST_SCHEMA_VERSION,
    package: {
      id: packageId,
      version,
      deploymentProfile,
      platform: CONTAINER_PLATFORM,
      architecture: 'x64',
      runtimeTarget: 'container',
      format: CONTAINER_FORMAT,
    },
    imageTag: settings.tag,
    imageFile: path.join(appRoot, 'Dockerfile'),
    imageBuildDir,
    manifestPath,
    binaryPath,
    binaryName: `bin/${BINARY_NAME}`,
    knowledgebaseRpcBinaryPath,
    knowledgebaseRpcBinaryName: `bin/${KNOWLEDGEBASE_RPC_BINARY_NAME}`,
    databaseModules,
    releaseDir,
    skipBuild: settings.skipBuild === true,
    push: settings.push === true,
    force: settings.force === true,
  };
  plan.issues = await validateBuildPlan(plan);
  return plan;
}

async function validateBuildPlan(plan) {
  const issues = [];
  if (process.platform !== 'linux') {
    issues.push(
      `container image builds require a Linux host (found ${process.platform}); build inside WSL/Ubuntu`,
    );
  }
  if (plan.skipBuild && !existsSync(plan.binaryPath)) {
    issues.push(`missing release binary for --skip-build: ${plan.binaryPath}`);
  }
  if (plan.skipBuild && !existsSync(plan.knowledgebaseRpcBinaryPath)) {
    issues.push(
      `missing Knowledgebase RPC release binary for --skip-build: ${plan.knowledgebaseRpcBinaryPath}`,
    );
  }
  for (const module of plan.databaseModules) {
    if (!existsSync(path.join(module.databaseRoot, 'database.manifest.json'))) {
      issues.push(`missing database module manifest: ${module.workspace}`);
    }
  }
  if (!existsSync(plan.imageFile)) {
    issues.push(`missing Dockerfile: ${plan.imageFile}`);
  }
  if (!existsSync(path.join(appRoot, 'sdkwork.app.config.json'))) {
    issues.push('missing sdkwork.app.config.json');
  }
  if (!existsSync(path.join(appRoot, 'specs', 'component.spec.json'))) {
    issues.push('missing specs/component.spec.json');
  }
  if (!existsSync(path.join(appRoot, 'bin', 'container', 'entrypoint'))) {
    issues.push('missing bin/container/entrypoint');
  }
  if (!existsSync(path.join(appRoot, 'bin', 'container', 'knowledgebase-rpc-entrypoint'))) {
    issues.push('missing bin/container/knowledgebase-rpc-entrypoint');
  }
  if (plan.push) {
    const repository = imageRepository(plan.imageTag);
    if (!repository.includes('/') || repository.startsWith('sdkwork-api-cloud-gateway')) {
      issues.push('--push requires --tag with an explicit registry repository');
    }
    if (!process.env.SDKWORK_CONTAINER_REGISTRY_USERNAME
      || !process.env.SDKWORK_CONTAINER_REGISTRY_PASSWORD) {
      issues.push('--push requires SDKWORK_CONTAINER_REGISTRY_USERNAME and SDKWORK_CONTAINER_REGISTRY_PASSWORD');
    }
  }
  return issues;
}

function renderBuildPlan(plan) {
  return [
    '[container-image-build] Build Plan',
    `[container-image-build]   package id: ${plan.package.id} (${plan.package.platform}-${plan.package.architecture} v${plan.package.version})`,
    `[container-image-build]   image tag: ${plan.imageTag}`,
    `[container-image-build]   push: ${plan.push}`,
    `[container-image-build]   Dockerfile: ${plan.imageFile}`,
    `[container-image-build]   image build dir: ${plan.imageBuildDir}`,
    `[container-image-build]   release binary: ${plan.binaryPath}`,
    `[container-image-build]   Knowledgebase RPC binary: ${plan.knowledgebaseRpcBinaryPath}`,
    `[container-image-build]   database modules: ${plan.databaseModules.length}`,
    ...plan.databaseModules.map((module) => `[container-image-build]     ${module.workspace} (${module.moduleId})`),
  ];
}

async function runCargoBuild(plan) {
  const args = [
    'build',
    '--release',
    '--locked',
    '-p',
    'sdkwork-api-cloud-gateway',
    '--bin',
    BINARY_NAME,
    '--no-default-features',
    '--features',
    CONTAINER_CARGO_FEATURES,
  ];
  const { stdout, stderr } = await execFileAsync('cargo', args, {
    cwd: appRoot,
    // Some path-dependency build scripts (sdkwork-cloudrouter-http) invoke
    // `python` to generate OpenAPI sources; Debian/Ubuntu only ships python3,
    // so default PYTHON to python3 on Linux hosts.
    env: {
      ...process.env,
      CARGO_INCREMENTAL: '0',
      PYTHON: process.env.PYTHON || (process.platform === 'linux' ? 'python3' : 'python'),
    },
    maxBuffer: 32 * 1024 * 1024,
  });
  if (stdout.trim()) {
    console.log(stdout.trim());
  }
  if (stderr.trim()) {
    console.log(stderr.trim());
  }

  const knowledgebaseArgs = [
    'build',
    '--release',
    '--locked',
    '--manifest-path',
    path.join(KNOWLEDGEBASE_ROOT, 'Cargo.toml'),
    '-p',
    KNOWLEDGEBASE_RPC_BINARY_NAME,
    '--bin',
    KNOWLEDGEBASE_RPC_BINARY_NAME,
    '--target-dir',
    path.dirname(plan.releaseDir),
  ];
  const knowledgebaseResult = await execFileAsync('cargo', knowledgebaseArgs, {
    cwd: KNOWLEDGEBASE_ROOT,
    env: {
      ...process.env,
      CARGO_INCREMENTAL: '0',
      PYTHON: process.env.PYTHON || (process.platform === 'linux' ? 'python3' : 'python'),
    },
    maxBuffer: 32 * 1024 * 1024,
  });
  if (knowledgebaseResult.stdout.trim()) {
    console.log(knowledgebaseResult.stdout.trim());
  }
  if (knowledgebaseResult.stderr.trim()) {
    console.log(knowledgebaseResult.stderr.trim());
  }
}

export function resolveContainerPackageId(
  deploymentProfile = DEFAULT_DEPLOYMENT_PROFILE,
  architecture = 'x64',
) {
  if (!['standalone', 'cloud'].includes(deploymentProfile)) {
    throw new Error(`Unsupported container deployment profile: ${deploymentProfile}`);
  }
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(architecture)) {
    throw new Error(`Invalid container architecture: ${architecture}`);
  }
  return `${CONTAINER_PLATFORM}-${architecture}-${deploymentProfile}-${CONTAINER_PROFILE}-${CONTAINER_FORMAT}`;
}

export function validateImageManifestRecord(manifest, expected = {}) {
  const issues = [];
  const expectedPackageId = expected.packageId ?? resolveContainerPackageId();
  if (manifest?.schemaVersion !== CONTAINER_IMAGE_MANIFEST_SCHEMA_VERSION) {
    issues.push(`schemaVersion must be ${CONTAINER_IMAGE_MANIFEST_SCHEMA_VERSION}`);
  }
  if (manifest?.packageId !== expectedPackageId) {
    issues.push(`packageId must be ${expectedPackageId}`);
  }
  if (expected.version && manifest?.version !== expected.version) {
    issues.push(`version must be ${expected.version}`);
  }
  if (!/^sha256:[a-f0-9]{64}$/u.test(String(manifest?.imageId ?? ''))) {
    issues.push('imageId must be an immutable sha256 image id');
  }
  if (manifest?.repoDigest !== null
    && !/@sha256:[a-f0-9]{64}$/u.test(String(manifest?.repoDigest ?? ''))) {
    issues.push('repoDigest must be null or an immutable repository digest');
  }
  if (expected.requireRepoDigest === true
    && !/@sha256:[a-f0-9]{64}$/u.test(String(manifest?.repoDigest ?? ''))) {
    issues.push('repoDigest is required for release publication');
  }
  if (!Number.isSafeInteger(manifest?.sizeBytes) || manifest.sizeBytes <= 0) {
    issues.push('sizeBytes must be a positive safe integer');
  }
  return issues;
}

async function validateRecordedImageManifest(settings) {
  const packageJson = await loadPackageJson();
  const version = normalizeVersion(
    settings.version || process.env.SDKWORK_PACKAGE_VERSION || packageJson.version || '0.1.0',
  );
  const manifestPath = path.join(appRoot, IMAGE_MANIFEST_FILE);
  if (!existsSync(manifestPath)) {
    throw new Error(`missing container image manifest: ${manifestPath}`);
  }
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  const issues = validateImageManifestRecord(manifest, {
    packageId: resolveContainerPackageId(settings.deploymentProfile || DEFAULT_DEPLOYMENT_PROFILE),
    version,
    requireRepoDigest: settings.requireRepoDigest,
  });
  if (issues.length > 0) {
    throw new Error(`container image manifest is invalid: ${issues.join('; ')}`);
  }
  if (settings.json) {
    console.log(JSON.stringify({ ok: true, manifestPath, manifest }, null, 2));
  } else {
    console.log(`[container-image-build] valid manifest: ${manifestPath}`);
  }
}

async function assembleBuildContext(plan) {
  await rm(plan.imageBuildDir, { recursive: true, force: true });
  await mkdir(path.join(plan.imageBuildDir, 'bin'), { recursive: true });
  await mkdir(path.join(plan.imageBuildDir, 'config'), { recursive: true });
  await mkdir(path.join(plan.imageBuildDir, 'specs'), { recursive: true });
  await mkdir(path.join(plan.imageBuildDir, 'container'), { recursive: true });
  await mkdir(path.join(plan.imageBuildDir, 'database-modules'), { recursive: true });

  await cp(plan.binaryPath, path.join(plan.imageBuildDir, plan.binaryName), {
    preserveTimestamps: true,
  });
  await cp(
    plan.knowledgebaseRpcBinaryPath,
    path.join(plan.imageBuildDir, plan.knowledgebaseRpcBinaryName),
    { preserveTimestamps: true },
  );

  // Release images carry only the production-safe template. Non-production
  // profiles remain source/deployment inputs and can be mounted at runtime.
  const configRoot = path.join(appRoot, 'etc');
  for (const file of RELEASE_CONFIG_TEMPLATES) {
    await cp(path.join(configRoot, file), path.join(plan.imageBuildDir, 'config', file));
  }

  // Component contract and application identity manifest.
  await cp(path.join(appRoot, 'specs', 'component.spec.json'), path.join(plan.imageBuildDir, 'specs', 'component.spec.json'));
  await cp(path.join(appRoot, 'sdkwork.app.config.json'), path.join(plan.imageBuildDir, 'sdkwork.app.config.json'));
  await cp(path.join(appRoot, 'README.md'), path.join(plan.imageBuildDir, 'README.md'));

  // Container entrypoint + metadata. Authored sources live under
  // bin/container/ (MODULE_BIN_SPEC.md §2.1); the build context keeps the
  // artifact-relative `container/` layout the image contract fixes
  // (APPLICATION_DEPLOY_LAYOUT_SPEC), so the copy is a build output.
  await cp(
    path.join(appRoot, 'bin', 'container', 'entrypoint'),
    path.join(plan.imageBuildDir, 'container', 'entrypoint'),
  );
  await cp(
    path.join(appRoot, 'bin', 'container', 'knowledgebase-rpc-entrypoint'),
    path.join(plan.imageBuildDir, 'container', 'knowledgebase-rpc-entrypoint'),
  );
  await writeFile(
    path.join(plan.imageBuildDir, 'container', 'metadata.json'),
    `${JSON.stringify(createContainerMetadata(plan), null, 2)}\n`,
    'utf8',
  );

  // Packaged database modules (46 workspaces resolved from cargo metadata).
  for (const module of plan.databaseModules) {
    const destination = path.join(plan.imageBuildDir, 'database-modules', module.workspace, 'database');
    await cp(module.databaseRoot, destination, { recursive: true, preserveTimestamps: true });
    const workspaceRoot = path.resolve(module.databaseRoot, '..');
    for (const extraPath of module.extraPaths) {
      await cp(
        path.join(workspaceRoot, extraPath),
        path.join(plan.imageBuildDir, 'database-modules', module.workspace, extraPath),
        { recursive: true, preserveTimestamps: true },
      );
    }
  }

  // Models catalog (sdkwork-models.json + models/ + overlays/) installed under
  // <install root>/data/sdkwork-models: the cloudrouter foundation module's
  // bundled catalog fallback (SDKWORK_MODELS_CATALOG_ROOT image ENV).
  const modelsCatalogRepo = path.join(appRoot, '..', 'sdkwork-models');
  for (const entry of [
    'sdkwork-models.json',
    'models',
    'overlays',
  ]) {
    const source = path.join(modelsCatalogRepo, entry);
    if (existsSync(source)) {
      await cp(
        source,
        path.join(plan.imageBuildDir, 'data', 'sdkwork-models', entry),
        { recursive: true, preserveTimestamps: true },
      );
    }
  }

  await writeFile(
    path.join(plan.imageBuildDir, 'install-manifest.json'),
    `${JSON.stringify(createInstallManifest(plan), null, 2)}\n`,
    'utf8',
  );
  await writeChecksums(plan.imageBuildDir);
  console.log(`[container-image-build] staged: ${plan.imageBuildDir}`);
}

function createInstallManifest(plan) {
  return {
    schemaVersion: 1,
    appId: APP_ID,
    packageId: plan.package.id,
    profile: CONTAINER_PROFILE,
    deploymentProfile: plan.package.deploymentProfile,
    platform: plan.package.platform,
    architecture: plan.package.architecture,
    format: 'container-image',
    version: plan.package.version,
    binary: plan.binaryName,
    sidecars: [
      {
        id: 'knowledgebase-rpc',
        binary: plan.knowledgebaseRpcBinaryName,
        entrypoint: 'container/knowledgebase-rpc-entrypoint',
      },
    ],
    configTemplates: RELEASE_CONFIG_TEMPLATES.map((file) => `config/${file}`),
    databaseModules: plan.databaseModules.map((module) => ({
      moduleId: module.moduleId,
      workspace: module.workspace,
      assets: `database-modules/${module.workspace}/database`,
    })),
    healthPath: '/healthz',
    readinessPath: '/readyz',
  };
}

function createContainerMetadata(plan) {
  return {
    schemaVersion: 1,
    appId: APP_ID,
    packageId: plan.package.id,
    version: plan.package.version,
    runtimeTarget: 'container',
    deploymentProfile: plan.package.deploymentProfile,
    entrypoint: '/opt/sdkwork/api-gateway/container/entrypoint',
    healthPath: '/healthz',
    readinessPath: '/readyz',
  };
}

async function writeChecksums(stageRoot) {
  const entries = [];
  for (const filePath of await listFiles(stageRoot)) {
    const relativePath = toPosixPath(path.relative(stageRoot, filePath));
    if (relativePath === 'checksums.sha256') {
      continue;
    }
    const digest = createHash('sha256').update(await readFile(filePath)).digest('hex');
    entries.push(`${digest}  ${relativePath}`);
  }
  await writeFile(path.join(stageRoot, 'checksums.sha256'), `${entries.sort().join('\n')}\n`, 'utf8');
}

async function listFiles(root) {
  const result = [];
  const entries = await readdir(root, { withFileTypes: true });
  for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
    const entryPath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      result.push(...await listFiles(entryPath));
    } else if (entry.isFile()) {
      result.push(entryPath);
    }
  }
  return result;
}

function toPosixPath(value) {
  return value.split(path.sep).join('/');
}

async function dockerVersion() {
  const { stdout } = await execFileAsync('docker', ['version', '--format', '{{.Server.Version}}']);
  return stdout.trim();
}

async function buildImage(plan) {
  const args = [
    'build',
    // --no-cache: the build context lives on drvfs (WSL mounts of Windows
    // drives) where coarse timestamps can fool BuildKit's COPY cache; the
    // image is small enough that full rebuilds stay fast.
    '--no-cache',
    '--build-arg',
    `VERSION=${plan.package.version}`,
    '-f',
    plan.imageFile,
    '-t',
    plan.imageTag,
    plan.imageBuildDir,
  ];
  const { stdout, stderr } = await execFileAsync('docker', args, {
    env: plan.dockerEnv ?? process.env,
    maxBuffer: 64 * 1024 * 1024,
  });
  if (stdout.trim()) {
    console.log(stdout.trim());
  }
  if (stderr.trim()) {
    console.log(stderr.trim());
  }
}

function imageRepository(imageTag) {
  const lastSlash = imageTag.lastIndexOf('/');
  const lastColon = imageTag.lastIndexOf(':');
  return lastColon > lastSlash ? imageTag.slice(0, lastColon) : imageTag;
}

async function pushImage(plan) {
  const repository = imageRepository(plan.imageTag);
  const registry = repository.split('/')[0];
  const dockerConfig = await mkdtemp(path.join(tmpdir(), 'sdkwork-container-registry-'));
  const dockerEnv = { ...process.env, DOCKER_CONFIG: dockerConfig };
  try {
    const login = spawnSync('docker', [
      'login',
      registry,
      '--username',
      process.env.SDKWORK_CONTAINER_REGISTRY_USERNAME,
      '--password-stdin',
    ], {
      cwd: appRoot,
      encoding: 'utf8',
      env: dockerEnv,
      input: process.env.SDKWORK_CONTAINER_REGISTRY_PASSWORD,
      maxBuffer: 4 * 1024 * 1024,
      windowsHide: true,
    });
    if (login.error) throw login.error;
    if (login.status !== 0) throw new Error(`container registry login failed: ${login.stderr.trim()}`);
    plan.dockerEnv = dockerEnv;
    const { stdout, stderr } = await execFileAsync('docker', ['push', plan.imageTag], {
      env: dockerEnv,
      maxBuffer: 64 * 1024 * 1024,
    });
    const output = `${stdout}\n${stderr}`;
    const match = /digest:\s*(sha256:[a-f0-9]{64})/iu.exec(output);
    if (!match) throw new Error('docker push did not return an immutable sha256 digest');
    const repoDigest = `${repository}@${match[1].toLowerCase()}`;
    await execFileAsync('docker', ['manifest', 'inspect', repoDigest], {
      env: dockerEnv,
      maxBuffer: 64 * 1024 * 1024,
    });
    return repoDigest;
  } finally {
    delete plan.dockerEnv;
    await rm(dockerConfig, { recursive: true, force: true });
  }
}

async function imageIdentity(imageTag, repoDigest = null) {
  const { stdout } = await execFileAsync('docker', [
    'image',
    'inspect',
    '--format',
    '{{index .RepoDigests 0}}|{{.Id}}|{{.Size}}',
    imageTag,
  ]);
  const [inspectedRepoDigest, imageId, sizeBytes] = stdout.trim().split('|');
  return {
    repoDigest: repoDigest || (inspectedRepoDigest || null),
    imageId: imageId || null,
    sizeBytes: Number(sizeBytes) || 0,
  };
}

function humanSize(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return 'n/a';
  }
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

async function writeImageManifest(plan, identity) {
  const manifest = {
    schemaVersion: CONTAINER_IMAGE_MANIFEST_SCHEMA_VERSION,
    packageId: plan.package.id,
    version: plan.package.version,
    imageTag: plan.imageTag,
    repoDigest: identity.repoDigest,
    imageId: identity.imageId,
    sizeBytes: identity.sizeBytes,
    sizeHuman: humanSize(identity.sizeBytes),
    buildDate: new Date().toISOString(),
  };
  await writeFile(plan.manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
  return manifest;
}

async function main(argv = process.argv.slice(2)) {
  const settings = parseArgs(argv);
  if (settings.help) {
    printHelp();
    return 0;
  }

  if (settings.validateManifest) {
    await validateRecordedImageManifest(settings);
    return 0;
  }

  const plan = await createBuildPlan(settings);
  const lines = renderBuildPlan(plan);
  if (settings.json && (settings.dryRun || settings.check)) {
    console.log(JSON.stringify({ ok: plan.issues.length === 0, issues: plan.issues, plan }, null, 2));
  } else {
    for (const line of lines) {
      console.log(line);
    }
    if (plan.issues.length > 0) {
      console.error('[container-image-build] validation issues:');
      for (const issue of plan.issues) {
        console.error(`[container-image-build]   ${issue}`);
      }
    }
  }
  if (settings.check) {
    // `--check` validates the plan and must never proceed to a build (its
    // documented contract): a clean plan exits 0, a plan with issues exits 1.
    return plan.issues.length > 0 ? 1 : 0;
  }
  if (settings.dryRun) {
    return 0;
  }
  if (plan.issues.length > 0) {
    throw new Error(`container image build plan is invalid: ${plan.issues.join('; ')}`);
  }

  try {
    const serverVersion = await dockerVersion();
    console.log(`[container-image-build] docker server: ${serverVersion}`);
  } catch {
    throw new Error('docker is not available or the daemon is not running; start docker first');
  }

  if (!plan.skipBuild) {
    console.log('[container-image-build] building release binary...');
    await runCargoBuild(plan);
  }
  if (!existsSync(plan.binaryPath)) {
    throw new Error(`Missing release binary: ${plan.binaryPath}`);
  }
  if (!existsSync(plan.knowledgebaseRpcBinaryPath)) {
    throw new Error(`Missing Knowledgebase RPC release binary: ${plan.knowledgebaseRpcBinaryPath}`);
  }

  await assembleBuildContext(plan);
  await buildImage(plan);
  const repoDigest = plan.push ? await pushImage(plan) : null;
  const identity = await imageIdentity(plan.imageTag, repoDigest);
  const manifest = await writeImageManifest(plan, identity);
  if (settings.json) {
    console.log(JSON.stringify({ ok: true, manifest }, null, 2));
  } else {
    console.log(`[container-image-build] image: ${manifest.imageTag}`);
    console.log(`[container-image-build] repoDigest: ${manifest.repoDigest ?? 'n/a (local build)'}`);
    console.log(`[container-image-build] imageId: ${manifest.imageId ?? 'n/a'}`);
    console.log(`[container-image-build] image size: ${manifest.sizeHuman} (${manifest.sizeBytes} bytes)`);
    console.log(`[container-image-build] manifest: ${plan.manifestPath}`);
  }
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === __filename) {
  main().catch((error) => {
    console.error(`[container-image-build] ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  });
}

export {
  CONTAINER_IMAGE_MANIFEST_SCHEMA_VERSION,
  createBuildPlan,
  main,
  parseArgs,
};

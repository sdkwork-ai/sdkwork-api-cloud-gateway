#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { chmod, cp, copyFile, mkdir, mkdtemp, readdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { createReadStream, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const APP_ID = 'sdkwork-api-cloud-gateway';
const BINARY_NAME = 'sdkwork-api-cloud-gateway';
const KNOWLEDGEBASE_RPC_BINARY_NAME = 'sdkwork-intelligence-knowledgebase-rpc-bin';
const SERVER_PROFILE = 'server';
const DEFAULT_DEPLOYMENT_PROFILE = 'standalone';
const SUPPORTED_FORMAT = 'tar.gz';
export const RELEASE_CONFIG_TEMPLATES = Object.freeze([
  'sdkwork-api-cloud-gateway.production.toml',
]);
export const DATABASE_ASSET_METADATA_ARGS = Object.freeze([
  'metadata',
  '--format-version',
  '1',
  '--locked',
  '--no-default-features',
  '--features',
  'platform-foundation-suite',
]);

// Container images omit foundation-webserver (host nginx / separate webserver
// product own reverse proxy). Keep server packages on the full suite.
export const CONTAINER_DATABASE_ASSET_METADATA_ARGS = Object.freeze([
  'metadata',
  '--format-version',
  '1',
  '--locked',
  '--no-default-features',
  '--features',
  'platform-foundation-suite-container',
]);

export const CONTAINER_CARGO_FEATURES = 'platform-foundation-suite-container';
export const SERVER_CARGO_FEATURES = 'platform-foundation-suite';

// Database workspaces whose module requires non-database assets next to
// `database/` under the packaged module root (mirrors sdkwork-cloudrouter's
// extra paths). sdkwork-iam materializes its module catalog from
// <module root>/iam/registry/iam-registry.config.json at bootstrap time.
const EXTRA_DATABASE_MODULE_PATHS = Object.freeze({
  'sdkwork-iam': ['iam'],
});
const scriptPath = fileURLToPath(import.meta.url);
const appRoot = path.resolve(path.dirname(scriptPath), '..');
const KNOWLEDGEBASE_ROOT = path.resolve(appRoot, '..', 'sdkwork-knowledgebase');

async function main() {
  const { command, options } = parseArgs(process.argv.slice(2));
  const context = await createPackageContext(options);

  if (command === 'package') {
    context.databaseModules = await resolveDatabaseAssetModules();
    await packageServer(context);
    return;
  }
  if (command === 'validate') {
    context.databaseModules = await resolveDatabaseAssetModules();
    await validateArchive(context);
    return;
  }
  if (command === 'help') {
    printHelp();
    return;
  }
  throw new Error(`Unsupported command: ${command}`);
}

function printHelp() {
  console.log(`Usage: node scripts/package-server.mjs <package|validate> [options]

Options:
  --version <value>       Package version. Defaults to SDKWORK_PACKAGE_VERSION or package.json.
  --platform <value>      Package platform. Defaults to SDKWORK_PACKAGE_PLATFORM or host platform.
  --arch <value>          Package architecture. Defaults to SDKWORK_PACKAGE_ARCHITECTURE or host arch.
  --format <value>        Package format. Only tar.gz is supported.
  --deployment-profile <value>  Deployment profile (standalone or cloud). Defaults to standalone.
  --package-id <value>    Package id. Defaults to SDKWORK_PACKAGE_ID or canonical id.
  --rust-target <value>   Optional cargo --target triple for cross-builds.
  --skip-build            Reuse an existing release binary.
`);
}

function parseArgs(argv) {
  const hasExplicitCommand = argv[0] && !argv[0].startsWith('-');
  const command = hasExplicitCommand ? argv[0] : 'package';
  const options = {};
  const start = hasExplicitCommand ? 1 : 0;

  for (let index = start; index < argv.length; index += 1) {
    const arg = argv[index];
    switch (arg) {
      case '--version':
        options.version = requireValue(argv, index, arg);
        index += 1;
        break;
      case '--platform':
        options.platform = requireValue(argv, index, arg);
        index += 1;
        break;
      case '--arch':
      case '--architecture':
        options.architecture = requireValue(argv, index, arg);
        index += 1;
        break;
      case '--format':
        options.format = requireValue(argv, index, arg);
        index += 1;
        break;
      case '--deployment-profile':
        options.deploymentProfile = requireValue(argv, index, arg);
        index += 1;
        break;
      case '--package-id':
        options.packageId = requireValue(argv, index, arg);
        index += 1;
        break;
      case '--rust-target':
        options.rustTarget = requireValue(argv, index, arg);
        index += 1;
        break;
      case '--skip-build':
        options.skipBuild = true;
        break;
      case '--help':
      case '-h':
        options.help = true;
        break;
      default:
        throw new Error(`Unsupported option: ${arg}`);
    }
  }

  return {
    command: options.help ? 'help' : command,
    options,
  };
}

function requireValue(argv, index, flag) {
  const value = argv[index + 1];
  if (value === undefined || value.startsWith('--')) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

async function createPackageContext(options) {
  const packageJson = JSON.parse(await readFile(path.join(appRoot, 'package.json'), 'utf8'));
  const hostPlatform = normalizeHostPlatform(process.platform);
  const hostArchitecture = normalizeHostArchitecture(process.arch);
  const platform = normalizePackagePlatform(
    options.platform ?? process.env.SDKWORK_PACKAGE_PLATFORM ?? hostPlatform
  );
  const architecture = normalizePackageArchitecture(
    options.architecture ?? process.env.SDKWORK_PACKAGE_ARCHITECTURE ?? hostArchitecture
  );
  const format = options.format ?? process.env.SDKWORK_PACKAGE_FORMAT ?? SUPPORTED_FORMAT;
  const deploymentProfile = resolveDeploymentProfile({
    explicit: options.deploymentProfile,
    standardEnvironment: process.env.SDKWORK_DEPLOYMENT_PROFILE,
    legacyEnvironment: process.env.SDKWORK_PACKAGE_DEPLOYMENT_PROFILE,
  });
  const version = normalizeVersion(
    options.version ?? process.env.SDKWORK_PACKAGE_VERSION ?? packageJson.version ?? '0.1.0'
  );
  const packageId =
    options.packageId ??
    process.env.SDKWORK_PACKAGE_ID ??
    `${platform}-${architecture}-${deploymentProfile}-${SERVER_PROFILE}-tar-gz`;
  const rustTarget = options.rustTarget ?? process.env.SDKWORK_RUST_TARGET ?? '';

  if (format !== SUPPORTED_FORMAT) {
    throw new Error(`Unsupported server package format ${format}; expected ${SUPPORTED_FORMAT}`);
  }
  if (!rustTarget && (platform !== hostPlatform || architecture !== hostArchitecture)) {
    throw new Error(
      `Refusing to label a native ${hostPlatform}/${hostArchitecture} build as ${platform}/${architecture}; pass --rust-target for cross-builds.`
    );
  }

  const distRoot = path.join(appRoot, 'dist', SERVER_PROFILE);
  const stageName = resolveServerStageName({
    architecture,
    deploymentProfile,
    platform,
    version,
  });
  const stageRoot = path.join(distRoot, stageName);
  const archivePath = path.join(distRoot, `${stageName}.${SUPPORTED_FORMAT}`);
  const binaryName = resolveExecutableName(BINARY_NAME, platform);
  const knowledgebaseRpcBinaryName = resolveExecutableName(
    KNOWLEDGEBASE_RPC_BINARY_NAME,
    platform,
  );
  const cargoTargetDir = resolveCargoTargetDir(appRoot, process.env.CARGO_TARGET_DIR);
  const releaseDir = resolveCargoReleaseDir(appRoot, cargoTargetDir, rustTarget);
  const binaryPath = path.join(releaseDir, binaryName);
  const knowledgebaseRpcBinaryPath = path.join(releaseDir, knowledgebaseRpcBinaryName);

  return {
    architecture,
    archivePath,
    binaryName,
    binaryPath,
    cargoTargetDir,
    knowledgebaseRpcBinaryName,
    knowledgebaseRpcBinaryPath,
    deploymentProfile,
    distRoot,
    format,
    hostArchitecture,
    hostPlatform,
    packageId,
    platform,
    rustTarget,
    skipBuild: options.skipBuild === true,
    stageName,
    stageRoot,
    version,
  };
}

export function resolveExecutableName(binaryName, platform) {
  return platform === 'windows' ? `${binaryName}.exe` : binaryName;
}

export function resolveCargoTargetDir(root, cargoTargetDir) {
  return String(cargoTargetDir ?? '').trim()
    ? path.resolve(root, String(cargoTargetDir).trim())
    : path.join(root, 'target');
}

export function resolveCargoReleaseDir(root, cargoTargetDir, rustTarget = '') {
  const targetRoot = resolveCargoTargetDir(root, cargoTargetDir);
  return rustTarget
    ? path.join(targetRoot, rustTarget, 'release')
    : path.join(targetRoot, 'release');
}

export function resolveServerStageName({ architecture, deploymentProfile, platform, version }) {
  return [
    APP_ID,
    version,
    platform,
    architecture,
    deploymentProfile,
    SERVER_PROFILE,
  ].join('-');
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

function normalizeHostPlatform(value) {
  if (value === 'win32') {
    return 'windows';
  }
  if (value === 'darwin') {
    return 'macos';
  }
  if (value === 'linux') {
    return 'linux';
  }
  throw new Error(`Unsupported host platform: ${value}`);
}

function normalizePackagePlatform(value) {
  const text = String(value ?? '').trim().toLowerCase();
  if (['linux', 'windows', 'macos'].includes(text)) {
    return text;
  }
  throw new Error(`Unsupported server package platform: ${value}`);
}

function normalizeHostArchitecture(value) {
  if (value === 'x64') {
    return 'x64';
  }
  if (value === 'arm64') {
    return 'arm64';
  }
  if (value === 'arm') {
    return 'armv7';
  }
  throw new Error(`Unsupported host architecture: ${value}`);
}

function normalizePackageArchitecture(value) {
  const text = String(value ?? '').trim().toLowerCase();
  if (['x64', 'arm64', 'armv7'].includes(text)) {
    return text;
  }
  throw new Error(`Unsupported server package architecture: ${value}`);
}

function normalizeDeploymentProfile(value) {
  const text = String(value ?? '').trim().toLowerCase();
  if (text === 'standalone' || text === 'cloud') {
    return text;
  }
  throw new Error(`Unsupported deployment profile: ${value}`);
}

export function resolveDeploymentProfile({
  explicit,
  standardEnvironment,
  legacyEnvironment,
} = {}) {
  return normalizeDeploymentProfile(
    explicit ?? standardEnvironment ?? legacyEnvironment ?? DEFAULT_DEPLOYMENT_PROFILE,
  );
}

async function packageServer(context) {
  if (!context.skipBuild) {
    runCargoBuild(context);
  }
  if (!existsSync(context.binaryPath)) {
    throw new Error(`Missing release binary: ${context.binaryPath}`);
  }
  if (!existsSync(context.knowledgebaseRpcBinaryPath)) {
    throw new Error(`Missing Knowledgebase RPC release binary: ${context.knowledgebaseRpcBinaryPath}`);
  }

  await assertPathInside(context.stageRoot, context.distRoot);
  await rm(context.stageRoot, { recursive: true, force: true });
  await rm(context.archivePath, { force: true });
  await rm(`${context.archivePath}.sha256`, { force: true });
  await mkdir(path.join(context.stageRoot, 'bin'), { recursive: true });
  await mkdir(path.join(context.stageRoot, 'config'), { recursive: true });
  await mkdir(path.join(context.stageRoot, 'specs'), { recursive: true });
  await mkdir(path.join(context.stageRoot, 'database-modules'), { recursive: true });
  await mkdir(path.join(context.stageRoot, 'install'), { recursive: true });
  await mkdir(path.join(context.stageRoot, 'data', 'sdkwork-models'), { recursive: true });

  await copyFile(context.binaryPath, path.join(context.stageRoot, 'bin', context.binaryName));
  await copyFile(
    context.knowledgebaseRpcBinaryPath,
    path.join(context.stageRoot, 'bin', context.knowledgebaseRpcBinaryName),
  );
  await copyRuntimeConfigTemplates(context.stageRoot);
  await copyDatabaseModuleAssets(context);
  await copyModelsCatalog(context.stageRoot);
  await copyInstallerAssets(context.stageRoot);
  await copyIfExists('README.md', path.join(context.stageRoot, 'README.md'));
  await copyIfExists('sdkwork.app.config.json', path.join(context.stageRoot, 'sdkwork.app.config.json'));
  await copyIfExists(path.join('specs', 'component.spec.json'), path.join(context.stageRoot, 'specs', 'component.spec.json'));
  await writeFile(path.join(context.stageRoot, 'INSTALL.md'), renderInstallGuide(context), 'utf8');
  await writeFile(
    path.join(context.stageRoot, 'install-manifest.json'),
    `${JSON.stringify(await createInstallManifest(context), null, 2)}\n`,
    'utf8'
  );
  await writeChecksums(context.stageRoot);
  await createArchive(context);
  await writeArtifactChecksum(context.archivePath);
  await validateArchive(context);

  console.log(`[sdkwork-api-cloud-gateway] packaged ${path.relative(appRoot, context.archivePath)}`);
}

function runCargoBuild(context) {
  const buildEnvironment = {
    ...process.env,
    CARGO_TARGET_DIR: context.cargoTargetDir,
    CARGO_INCREMENTAL: '0',
    PYTHON: process.env.PYTHON || (process.platform === 'linux' ? 'python3' : 'python'),
  };
  for (const command of createCargoBuildCommands(context)) {
    run('cargo', command.args, {
      cwd: command.cwd,
      env: buildEnvironment,
    });
  }
}

export function createCargoBuildCommands(context) {
  const gatewayArgs = [
    'build',
    '--release',
    '--locked',
    '-p',
    'sdkwork-api-cloud-gateway',
    '--bin',
    BINARY_NAME,
    '--no-default-features',
    '--features',
    'platform-foundation-suite',
  ];
  const knowledgebaseArgs = [
    'build',
    '--release',
    '--locked',
    '-p',
    KNOWLEDGEBASE_RPC_BINARY_NAME,
    '--bin',
    KNOWLEDGEBASE_RPC_BINARY_NAME,
  ];
  if (context.rustTarget) {
    gatewayArgs.push('--target', context.rustTarget);
    knowledgebaseArgs.push('--target', context.rustTarget);
  }
  return [
    { args: gatewayArgs, cwd: appRoot },
    { args: knowledgebaseArgs, cwd: KNOWLEDGEBASE_ROOT },
  ];
}

async function copyRuntimeConfigTemplates(stageRoot) {
  const configRoot = path.join(appRoot, 'etc');
  for (const file of RELEASE_CONFIG_TEMPLATES) {
    await copyFile(path.join(configRoot, file), path.join(stageRoot, 'config', file));
  }
}

async function resolveDatabaseAssetModules() {
  const metadata = JSON.parse(run(
    'cargo',
    DATABASE_ASSET_METADATA_ARGS,
    { cwd: appRoot, capture: true },
  ));
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

export function resolveDatabaseHostCratesFromMetadata(metadata) {
  const gateway = metadata?.packages?.find((candidate) =>
    candidate.name === BINARY_NAME
      && path.resolve(candidate.manifest_path) === path.join(appRoot, 'crates', BINARY_NAME, 'Cargo.toml')
  );
  if (!gateway) {
    throw new Error(`Cargo metadata is missing ${BINARY_NAME}`);
  }
  const directHosts = gateway.dependencies.filter((dependency) =>
    dependency.name.endsWith('-database-host') && typeof dependency.path === 'string'
  );
  if (directHosts.length > 0) {
    throw new Error(
      `${BINARY_NAME} must consume database lifecycle through owner API assemblies, not direct database-host dependencies: ${directHosts.map((dependency) => dependency.name).sort().join(', ')}`,
    );
  }
  if (!metadata.resolve || !Array.isArray(metadata.resolve.nodes)) {
    throw new Error('Cargo metadata must include the resolved production dependency graph');
  }

  const nodesById = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const reachablePackageIds = new Set();
  const pendingPackageIds = [gateway.id];
  while (pendingPackageIds.length > 0) {
    const packageId = pendingPackageIds.pop();
    if (!packageId || reachablePackageIds.has(packageId)) {
      continue;
    }
    reachablePackageIds.add(packageId);
    const node = nodesById.get(packageId);
    for (const dependency of node?.deps ?? []) {
      const dependencyKinds = dependency.dep_kinds ?? [];
      const isRuntimeDependency = dependencyKinds.length === 0
        || dependencyKinds.some((entry) => entry.kind !== 'dev');
      if (isRuntimeDependency) {
        pendingPackageIds.push(dependency.pkg);
      }
    }
  }

  const hosts = metadata.packages
    .filter((candidate) =>
      reachablePackageIds.has(candidate.id)
      && candidate.source === null
      && candidate.name.endsWith('-database-host')
      && typeof candidate.manifest_path === 'string'
    )
    .map((candidate) => ({
      name: candidate.name,
      crateRoot: path.dirname(candidate.manifest_path),
    }))
    .sort((left, right) => left.name.localeCompare(right.name));
  if (hosts.length === 0) {
    throw new Error(`${BINARY_NAME} production assembly closure has no database-host dependencies`);
  }
  return hosts;
}

async function copyDatabaseModuleAssets(context) {
  for (const module of context.databaseModules) {
    const destination = path.join(
      context.stageRoot,
      'database-modules',
      module.workspace,
      'database',
    );
    await cp(module.databaseRoot, destination, { recursive: true });
    const workspaceRoot = path.resolve(module.databaseRoot, '..');
    for (const extraPath of module.extraPaths) {
      await cp(
        path.join(workspaceRoot, extraPath),
        path.join(context.stageRoot, 'database-modules', module.workspace, extraPath),
        { recursive: true },
      );
    }
  }
}

async function copyModelsCatalog(stageRoot) {
  const modelsRoot = path.join(appRoot, '..', 'sdkwork-models');
  for (const entry of ['sdkwork-models.json', 'models', 'overlays']) {
    const source = path.join(modelsRoot, entry);
    if (existsSync(source)) {
      await cp(source, path.join(stageRoot, 'data', 'sdkwork-models', entry), {
        recursive: true,
      });
    }
  }
}

async function copyInstallerAssets(stageRoot) {
  const installRoot = path.join(stageRoot, 'install');
  const installer = path.join(installRoot, 'install-ubuntu.sh');
  const knowledgebaseRpcEntrypoint = path.join(installRoot, 'knowledgebase-rpc-entrypoint');
  await copyFile(path.join(appRoot, 'scripts', 'install-ubuntu.sh'), installer);
  await copyFile(
    path.join(appRoot, 'container', 'knowledgebase-rpc-entrypoint'),
    knowledgebaseRpcEntrypoint,
  );
  await copyFile(
    path.join(appRoot, 'deployments', 'systemd', 'sdkwork-api-gateway.service'),
    path.join(installRoot, 'sdkwork-api-gateway.service'),
  );
  await copyFile(
    path.join(appRoot, 'deployments', 'systemd', 'sdkwork-knowledgebase-rpc.service'),
    path.join(installRoot, 'sdkwork-knowledgebase-rpc.service'),
  );
  await chmod(installer, 0o755);
  await chmod(knowledgebaseRpcEntrypoint, 0o755);
}

async function copyIfExists(relativeSource, destination) {
  const source = path.join(appRoot, relativeSource);
  if (existsSync(source)) {
    await copyFile(source, destination);
  }
}

function renderInstallGuide(context) {
  const binaryPath = context.platform === 'windows'
    ? '.\\bin\\sdkwork-api-cloud-gateway.exe'
    : './bin/sdkwork-api-cloud-gateway';
  // Service registration name (PACKAGING_SPEC §5.5): sdkwork-<application-code>
  // for both systemd units and Windows SCM services.
  const serviceName = 'sdkwork-api-gateway';
  return `# SDKWork API Gateway Server Package

Package: ${context.packageId}
Version: ${context.version}
Target: ${context.platform}/${context.architecture}

## Start (development / smoke test)

\`\`\`sh
${binaryPath} --config config/sdkwork-api-cloud-gateway.production.toml
\`\`\`

## Health Checks

\`\`\`sh
curl http://127.0.0.1:3900/healthz
curl http://127.0.0.1:3900/readyz
\`\`\`

## Production deployment (systemd service)

Service deployments install the binary and runtime layout per
RUNTIME_DIRECTORY_SPEC and enable the \`${serviceName}\` systemd service
(\`/usr/lib/systemd/system/${serviceName}.service\`, PACKAGING_SPEC §5.5):

1. Materialize \`config/sdkwork-api-cloud-gateway.production.toml\` at the
   canonical config path \`/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml\`
   (\`root:sdkwork\`, 0640).
2. Put installation-generated overrides in
   \`/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.env\` (0640) — never
   embed them in the unit file.
3. Inject the unified PostgreSQL profile through \`SDKWORK_DATABASE_*\` with
   password material via \`password_file\` or
   \`/etc/sdkwork/database/database.secret\`; never write credentials into env
   files, unit files, or config templates (PACKAGING_SPEC §5.5).
4. The package installs and enables the bundled
   \`sdkwork-knowledgebase-rpc.service\` before the gateway service.
5. \`sudo systemctl enable --now ${serviceName}\`

See docs/guides/operator/docker-install.md for the complete per-OS
installation and configuration standards (Linux/macOS/Windows/container).
`;
}

async function createInstallManifest(context) {
  return {
    schemaVersion: 1,
    appId: APP_ID,
    packageId: context.packageId,
    profile: SERVER_PROFILE,
    deploymentProfile: context.deploymentProfile,
    platform: context.platform,
    architecture: context.architecture,
    format: context.format,
    version: context.version,
    binary: `bin/${context.binaryName}`,
    sidecars: [
      {
        serviceName: 'sdkwork-knowledgebase-rpc',
        binary: `bin/${context.knowledgebaseRpcBinaryName}`,
        systemdUnit: '/usr/lib/systemd/system/sdkwork-knowledgebase-rpc.service',
      },
    ],
    configTemplates: RELEASE_CONFIG_TEMPLATES.map((file) => `config/${file}`),
    databaseModules: context.databaseModules.map((module) => ({
      moduleId: module.moduleId,
      workspace: module.workspace,
      assets: `database-modules/${module.workspace}/database`,
    })),
    // Host directory mapping and service registration (PACKAGING_SPEC §5.5,
    // RUNTIME_DIRECTORY_SPEC §4.1-§4.5). The package declares where each asset
    // role lands so installers and automation can materialize the canonical
    // layout without re-deriving it.
    installConfiguration: {
      serviceName: 'sdkwork-api-gateway',
      systemdUnit: '/usr/lib/systemd/system/sdkwork-api-gateway.service',
      requiredServices: ['sdkwork-knowledgebase-rpc.service'],
      directories: {
        config: '/etc/sdkwork/api-gateway',
        data: '/var/lib/sdkwork/api-gateway',
        logs: '/var/log/sdkwork/api-gateway',
        cache: '/var/cache/sdkwork/api-gateway',
        run: '/run/sdkwork/api-gateway',
        installRoot: '/opt/sdkwork/api-gateway',
      },
      configFile: '/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml',
      envFile: '/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.env',
      secretsDir: '/etc/sdkwork/api-gateway',
      container: {
        configMount: '/etc/sdkwork/api-gateway',
        secretsMount: '/run/secrets/sdkwork/api-gateway',
        dataVolume: '/var/lib/sdkwork/api-gateway',
      },
      database: {
        canonicalName: 'sdkwork_ai_prod',
        workspaceConfig: '/etc/sdkwork/database/database.toml',
        workspaceSecret: '/etc/sdkwork/database/database.secret',
      },
    },
    healthPath: '/healthz',
    readinessPath: '/readyz',
    contentManifestExclusions: [
      'install-manifest.json',
      'checksums.sha256',
    ],
    files: await describeFiles(context.stageRoot),
  };
}

async function describeFiles(stageRoot) {
  const files = [];
  for (const filePath of await listFiles(stageRoot)) {
    const relativePath = toPosixPath(path.relative(stageRoot, filePath));
    const fileStats = await stat(filePath);
    files.push({
      path: relativePath,
      size: fileStats.size,
      sha256: createHash('sha256').update(await readFile(filePath)).digest('hex'),
    });
  }
  return files;
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
    } else {
      throw new Error(`Release package contains an unsupported filesystem entry: ${entryPath}`);
    }
  }
  return result;
}

async function createArchive(context) {
  await mkdir(context.distRoot, { recursive: true });
  const archiveName = resolveTarArchiveName(context.distRoot, context.archivePath);
  run('tar', ['-czf', archiveName, context.stageName], { cwd: context.distRoot });
}

async function validateArchive(context) {
  if (!existsSync(context.archivePath)) {
    throw new Error(`Missing server archive: ${context.archivePath}`);
  }
  const archiveStats = await stat(context.archivePath);
  if (archiveStats.size <= 0) {
    throw new Error(`Server archive is empty: ${context.archivePath}`);
  }
  await verifyArtifactChecksum(context.archivePath);

  const archiveName = resolveTarArchiveName(context.distRoot, context.archivePath);
  const listing = run('tar', ['-tzf', archiveName], {
    cwd: context.distRoot,
    capture: true,
  });
  const archiveEntries = validateArchiveEntryPaths(listing, context.stageName);
  const requiredEntries = [
    `${context.stageName}/bin/${context.binaryName}`,
    `${context.stageName}/bin/${context.knowledgebaseRpcBinaryName}`,
    `${context.stageName}/config/sdkwork-api-cloud-gateway.production.toml`,
    `${context.stageName}/install/install-ubuntu.sh`,
    `${context.stageName}/install/sdkwork-api-gateway.service`,
    `${context.stageName}/install/sdkwork-knowledgebase-rpc.service`,
    `${context.stageName}/install/knowledgebase-rpc-entrypoint`,
    `${context.stageName}/install-manifest.json`,
    `${context.stageName}/checksums.sha256`,
    `${context.stageName}/README.md`,
    ...context.databaseModules.map(
      (module) => `${context.stageName}/database-modules/${module.workspace}/database/database.manifest.json`,
    ),
  ];

  for (const entry of requiredEntries) {
    if (!archiveEntries.has(entry)) {
      throw new Error(`Server archive missing ${entry}`);
    }
  }
  for (const environment of ['development', 'staging', 'test']) {
    const forbiddenEntry = `${context.stageName}/config/sdkwork-api-cloud-gateway.${environment}.toml`;
    if (archiveEntries.has(forbiddenEntry)) {
      throw new Error(`Server archive contains non-production config ${forbiddenEntry}`);
    }
  }

  const extractionRoot = await mkdtemp(path.join(tmpdir(), `${APP_ID}-validate-`));
  try {
    run('tar', ['-xzf', archiveName, '-C', extractionRoot], {
      cwd: context.distRoot,
    });
    await verifyStageChecksums(path.join(extractionRoot, context.stageName));
  } finally {
    await rm(extractionRoot, { recursive: true, force: true });
  }
  console.log(`[sdkwork-api-cloud-gateway] validated ${path.relative(appRoot, context.archivePath)}`);
}

async function sha256File(filePath) {
  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    const stream = createReadStream(filePath);
    stream.on('error', reject);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.on('end', () => resolve(hash.digest('hex')));
  });
}

export async function writeArtifactChecksum(artifactPath) {
  const digest = await sha256File(artifactPath);
  await writeFile(
    `${artifactPath}.sha256`,
    `${digest}  ${path.basename(artifactPath)}\n`,
    'utf8',
  );
  return digest;
}

export async function verifyArtifactChecksum(artifactPath) {
  const checksumPath = `${artifactPath}.sha256`;
  if (!existsSync(checksumPath)) {
    throw new Error(`Missing artifact checksum: ${checksumPath}`);
  }
  const checksumText = await readFile(checksumPath, 'utf8');
  const match = /^([a-f0-9]{64})  ([^\r\n]+)\r?\n?$/u.exec(checksumText);
  if (!match || match[2] !== path.basename(artifactPath)) {
    throw new Error(`Invalid artifact checksum record: ${checksumPath}`);
  }
  const actualDigest = await sha256File(artifactPath);
  if (actualDigest !== match[1]) {
    throw new Error(`Artifact checksum mismatch: ${artifactPath}`);
  }
  return actualDigest;
}

export function validateArchiveEntryPaths(listing, stageName) {
  const entries = new Set();
  for (const rawEntry of listing.split(/\r?\n/u)) {
    const entry = rawEntry.endsWith('/') ? rawEntry.slice(0, -1) : rawEntry;
    if (!entry) {
      continue;
    }
    const segments = entry.split('/');
    if (
      entry.includes('\\')
      || path.posix.isAbsolute(entry)
      || segments[0] !== stageName
      || segments.some((segment) => !segment || segment === '.' || segment === '..')
    ) {
      throw new Error(`Server archive contains an unsafe entry: ${rawEntry}`);
    }
    entries.add(entry);
  }
  return entries;
}

export async function verifyStageChecksums(stageRoot) {
  const checksumPath = path.join(stageRoot, 'checksums.sha256');
  const checksumText = await readFile(checksumPath, 'utf8');
  const expected = new Map();

  for (const line of checksumText.split(/\r?\n/u)) {
    if (!line) {
      continue;
    }
    const match = /^([a-f0-9]{64})  ([^\r\n]+)$/u.exec(line);
    if (!match) {
      throw new Error(`Invalid checksums.sha256 entry: ${line}`);
    }
    const [, digest, relativePath] = match;
    const segments = relativePath.split('/');
    if (
      relativePath === 'checksums.sha256'
      || relativePath.includes('\\')
      || path.posix.isAbsolute(relativePath)
      || segments.some((segment) => !segment || segment === '.' || segment === '..')
    ) {
      throw new Error(`Unsafe checksums.sha256 path: ${relativePath}`);
    }
    if (expected.has(relativePath)) {
      throw new Error(`Duplicate checksums.sha256 path: ${relativePath}`);
    }
    expected.set(relativePath, digest);
  }

  const actualFiles = await listFiles(stageRoot);
  const actualPaths = new Set();
  for (const filePath of actualFiles) {
    const relativePath = toPosixPath(path.relative(stageRoot, filePath));
    if (relativePath === 'checksums.sha256') {
      continue;
    }
    actualPaths.add(relativePath);
    const expectedDigest = expected.get(relativePath);
    if (!expectedDigest) {
      throw new Error(`checksums.sha256 is missing ${relativePath}`);
    }
    const actualDigest = createHash('sha256').update(await readFile(filePath)).digest('hex');
    if (actualDigest !== expectedDigest) {
      throw new Error(`Checksum mismatch for ${relativePath}`);
    }
  }
  for (const relativePath of expected.keys()) {
    if (!actualPaths.has(relativePath)) {
      throw new Error(`checksums.sha256 references missing file ${relativePath}`);
    }
  }
}

export function resolveTarArchiveName(distRoot, archivePath) {
  const relativePath = path.relative(distRoot, archivePath);
  if (
    relativePath.length === 0 ||
    relativePath.startsWith('..') ||
    path.isAbsolute(relativePath)
  ) {
    throw new Error(`Archive path must stay inside ${distRoot}: ${archivePath}`);
  }
  return relativePath;
}

function run(command, args, { cwd, capture = false, env } = {}) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: 'utf8',
    env: env ?? process.env,
    maxBuffer: 64 * 1024 * 1024,
    shell: false,
    stdio: capture ? ['ignore', 'pipe', 'pipe'] : 'inherit',
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    const output = [result.stdout, result.stderr].filter(Boolean).join('\n').trim();
    throw new Error(`${command} ${args.join(' ')} failed with exit code ${result.status}${output ? `\n${output}` : ''}`);
  }
  return capture ? String(result.stdout ?? '') : '';
}

async function assertPathInside(targetPath, parentPath) {
  const relativePath = path.relative(parentPath, targetPath);
  if (relativePath.startsWith('..') || path.isAbsolute(relativePath)) {
    throw new Error(`Refusing to write outside ${parentPath}: ${targetPath}`);
  }
}

function toPosixPath(value) {
  return value.split(path.sep).join('/');
}

if (process.argv[1] && path.resolve(process.argv[1]) === scriptPath) {
  main().catch((error) => {
    console.error(`[sdkwork-api-cloud-gateway] ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  });
}

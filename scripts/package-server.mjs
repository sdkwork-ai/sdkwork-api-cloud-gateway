#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { cp, copyFile, mkdir, readdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const APP_ID = 'sdkwork-api-cloud-gateway';
const BINARY_NAME = 'sdkwork-api-cloud-gateway';
const SERVER_PROFILE = 'server';
const DEFAULT_DEPLOYMENT_PROFILE = 'standalone';
const SUPPORTED_FORMAT = 'tar.gz';

// Database workspaces whose module requires non-database assets next to
// `database/` under the packaged module root (mirrors sdkwork-cloudrouter's
// extra paths). sdkwork-iam materializes its module catalog from
// <module root>/iam/registry/iam-registry.config.json at bootstrap time.
const EXTRA_DATABASE_MODULE_PATHS = Object.freeze({
  'sdkwork-iam': ['iam'],
});

const scriptPath = fileURLToPath(import.meta.url);
const appRoot = path.resolve(path.dirname(scriptPath), '..');

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
  const binaryName = platform === 'windows' ? `${BINARY_NAME}.exe` : BINARY_NAME;
  const releaseDir = resolveCargoReleaseDir(
    appRoot,
    process.env.CARGO_TARGET_DIR,
    rustTarget,
  );
  const binaryPath = path.join(releaseDir, binaryName);

  return {
    architecture,
    archivePath,
    binaryName,
    binaryPath,
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

export function resolveCargoReleaseDir(root, cargoTargetDir, rustTarget = '') {
  const targetRoot = String(cargoTargetDir ?? '').trim()
    ? path.resolve(root, String(cargoTargetDir).trim())
    : path.join(root, 'target');
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

  await assertPathInside(context.stageRoot, context.distRoot);
  await rm(context.stageRoot, { recursive: true, force: true });
  await rm(context.archivePath, { force: true });
  await mkdir(path.join(context.stageRoot, 'bin'), { recursive: true });
  await mkdir(path.join(context.stageRoot, 'config'), { recursive: true });
  await mkdir(path.join(context.stageRoot, 'specs'), { recursive: true });
  await mkdir(path.join(context.stageRoot, 'database-modules'), { recursive: true });

  await copyFile(context.binaryPath, path.join(context.stageRoot, 'bin', context.binaryName));
  await copyRuntimeConfigTemplates(context.stageRoot);
  await copyDatabaseModuleAssets(context);
  await copyIfExists('README.md', path.join(context.stageRoot, 'README.md'));
  await copyIfExists('sdkwork.app.config.json', path.join(context.stageRoot, 'sdkwork.app.config.json'));
  await copyIfExists(path.join('specs', 'component.spec.json'), path.join(context.stageRoot, 'specs', 'component.spec.json'));
  await writeFile(path.join(context.stageRoot, 'INSTALL.md'), renderInstallGuide(context), 'utf8');
  await writeFile(
    path.join(context.stageRoot, 'install-manifest.json'),
    `${JSON.stringify(createInstallManifest(context), null, 2)}\n`,
    'utf8'
  );
  await writeChecksums(context.stageRoot);
  await createArchive(context);
  await validateArchive(context);

  console.log(`[sdkwork-api-cloud-gateway] packaged ${path.relative(appRoot, context.archivePath)}`);
}

function runCargoBuild(context) {
  const args = [
    'build',
    '--release',
    '-p',
    'sdkwork-api-cloud-gateway',
    '--bin',
    BINARY_NAME,
    '--no-default-features',
    '--features',
    'platform-foundation-suite',
  ];
  if (context.rustTarget) {
    args.push('--target', context.rustTarget);
  }
  run('cargo', args, {
    cwd: appRoot,
    env: { ...process.env, CARGO_INCREMENTAL: '0' },
  });
}

async function copyRuntimeConfigTemplates(stageRoot) {
  const configRoot = path.join(appRoot, 'etc');
  const files = await readdir(configRoot);
  for (const file of files.filter((item) => item.endsWith('.toml')).sort()) {
    await copyFile(path.join(configRoot, file), path.join(stageRoot, 'config', file));
  }
}

async function resolveDatabaseAssetModules() {
  const metadata = JSON.parse(run(
    'cargo',
    ['metadata', '--format-version', '1', '--locked'],
    { cwd: appRoot, capture: true },
  ));
  const hostCrates = resolveDatabaseHostCratesFromMetadata(metadata);
  const modules = [];

  for (const hostCrate of hostCrates) {
    const workspaceRoot = path.resolve(hostCrate.crateRoot, '..', '..');
    const workspace = path.basename(workspaceRoot);
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

  const hosts = gateway.dependencies
    .filter((dependency) =>
      dependency.name.endsWith('-database-host') && typeof dependency.path === 'string'
    )
    .map((dependency) => ({ name: dependency.name, crateRoot: dependency.path }));
  if (hosts.length === 0) {
    throw new Error(`${BINARY_NAME} has no convention-named database host dependencies`);
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
4. \`sudo systemctl enable --now ${serviceName}\`

See docs/guides/operator/INSTALLATION_STANDARDS.md for the complete per-OS
installation and configuration standards (Linux/macOS/Windows/container).
`;
}

function createInstallManifest(context) {
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
    configTemplates: [
      'config/sdkwork-api-cloud-gateway.development.toml',
      'config/sdkwork-api-cloud-gateway.production.toml',
      'config/sdkwork-api-cloud-gateway.test.toml',
    ],
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

async function createArchive(context) {
  await mkdir(context.distRoot, { recursive: true });
  // --force-local keeps GNU tar on Windows from treating the drive letter
  // in absolute archive paths as a remote host (E: -> host "E").
  run('tar', ['--force-local', '-czf', context.archivePath, '-C', context.distRoot, context.stageName], { cwd: appRoot });
}

async function validateArchive(context) {
  if (!existsSync(context.archivePath)) {
    throw new Error(`Missing server archive: ${context.archivePath}`);
  }
  const archiveStats = await stat(context.archivePath);
  if (archiveStats.size <= 0) {
    throw new Error(`Server archive is empty: ${context.archivePath}`);
  }

  const listing = run('tar', ['--force-local', '-tzf', context.archivePath], { cwd: appRoot, capture: true });
  const requiredEntries = [
    `${context.stageName}/bin/${context.binaryName}`,
    `${context.stageName}/config/sdkwork-api-cloud-gateway.production.toml`,
    `${context.stageName}/install-manifest.json`,
    `${context.stageName}/checksums.sha256`,
    `${context.stageName}/README.md`,
    ...context.databaseModules.map(
      (module) => `${context.stageName}/database-modules/${module.workspace}/database/database.manifest.json`,
    ),
  ];

  for (const entry of requiredEntries) {
    if (!listing.includes(entry)) {
      throw new Error(`Server archive missing ${entry}`);
    }
  }
  console.log(`[sdkwork-api-cloud-gateway] validated ${path.relative(appRoot, context.archivePath)}`);
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

#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import {
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from 'node:fs/promises';
import { existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const CONTAINER_MANIFEST_PATH = 'dist/container-image.json';
const SHA256_PATTERN = /^sha256:([a-f0-9]{64})$/u;
const OCI_DIGEST_PATTERN = /^([^\s@]+)@(sha256:[a-f0-9]{64})$/u;

function printHelp() {
  console.log(`Usage: node scripts/release-evidence.mjs <sign|sbom|evidence|verify> [options]

The active package is selected from SDKWORK workflow environment variables.
Signing requires SDKWORK_RELEASE_SIGNING_PRIVATE_KEY. Container evidence also
requires dist/container-image.json to contain a pushed repository digest.
`);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? appRoot,
    encoding: 'utf8',
    env: options.env ?? process.env,
    input: options.input,
    maxBuffer: 64 * 1024 * 1024,
    windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} failed: ${(result.stderr || result.stdout || '').trim()}`);
  }
  return String(result.stdout ?? '').trim();
}

function requiredEnv(name, env = process.env) {
  const value = String(env[name] ?? '').trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function normalizeVersion(value) {
  const text = String(value ?? '').trim().replace(/^refs\/tags\//u, '').replace(/^v(?=\d)/u, '');
  if (!/^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/u.test(text)) {
    throw new Error(`SDKWORK_PACKAGE_VERSION must use SemVer: ${value}`);
  }
  return text;
}

function safeRelativePath(value, label) {
  const normalized = String(value ?? '').replaceAll('\\', '/');
  if (!normalized || path.isAbsolute(normalized) || /^[A-Za-z]:\//u.test(normalized)) {
    throw new Error(`${label} must be a safe relative path`);
  }
  if (normalized.split('/').some((segment) => segment === '..' || segment === '')) {
    throw new Error(`${label} must be a safe relative path`);
  }
  return normalized;
}

function toRelative(filePath) {
  return path.relative(appRoot, filePath).split(path.sep).join('/');
}

async function sha256File(filePath) {
  return `sha256:${createHash('sha256').update(await readFile(filePath)).digest('hex')}`;
}

async function readContainerManifest(manifestPath) {
  let manifest;
  try {
    manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  } catch (error) {
    throw new Error(`invalid container image manifest ${manifestPath}: ${error.message}`);
  }
  const match = OCI_DIGEST_PATTERN.exec(String(manifest.repoDigest ?? ''));
  if (!match) {
    throw new Error('container release evidence requires a pushed repository@sha256 digest; a local image ID is not sufficient');
  }
  return { manifest, repository: match[1], digest: match[2], repoDigest: match[0] };
}

export async function resolveArtifactBinding(env = process.env) {
  const runtimeTarget = requiredEnv('SDKWORK_RUNTIME_TARGET', env);
  const packageId = requiredEnv('SDKWORK_PACKAGE_ID', env);
  const version = normalizeVersion(requiredEnv('SDKWORK_PACKAGE_VERSION', env));
  const deploymentProfile = requiredEnv('SDKWORK_DEPLOYMENT_PROFILE', env);
  if (!['standalone', 'cloud'].includes(deploymentProfile)) {
    throw new Error(`unsupported deployment profile: ${deploymentProfile}`);
  }
  if (runtimeTarget === 'container') {
    const artifactPath = safeRelativePath(
      env.SDKWORK_PACKAGE_ARTIFACT_PATH || CONTAINER_MANIFEST_PATH,
      'SDKWORK_PACKAGE_ARTIFACT_PATH',
    );
    const absoluteArtifactPath = path.resolve(appRoot, artifactPath);
    const image = await readContainerManifest(absoluteArtifactPath);
    if (image.manifest.packageId !== packageId) throw new Error('container manifest packageId does not match SDKWORK_PACKAGE_ID');
    if (image.manifest.version !== version) throw new Error('container manifest version does not match SDKWORK_PACKAGE_VERSION');
    return {
      artifactKind: 'oci-image',
      artifactPath,
      absoluteArtifactPath,
      digest: image.digest,
      locator: image.repoDigest,
      packageId,
      version,
      runtimeTarget,
      deploymentProfile,
      signablePath: path.resolve(appRoot, 'dist/container-image.digest'),
      signaturePath: path.resolve(appRoot, 'dist/container-image.sig'),
      publicKeyPath: path.resolve(appRoot, 'dist/container-image.pub.pem'),
      sbomPath: path.resolve(appRoot, 'dist/container-image.cdx.json'),
      provenancePath: path.resolve(appRoot, 'dist/container-image.intoto.jsonl'),
    };
  }

  if (runtimeTarget !== 'server') throw new Error(`unsupported release evidence runtime target: ${runtimeTarget}`);
  const artifactPath = safeRelativePath(requiredEnv('SDKWORK_PACKAGE_ARTIFACT_PATH', env), 'SDKWORK_PACKAGE_ARTIFACT_PATH');
  const absoluteArtifactPath = path.resolve(appRoot, artifactPath);
  if (!existsSync(absoluteArtifactPath)) throw new Error(`package artifact does not exist: ${artifactPath}`);
  return {
    artifactKind: 'file',
    artifactPath,
    absoluteArtifactPath,
    digest: await sha256File(absoluteArtifactPath),
    locator: artifactPath,
    packageId,
    version,
    runtimeTarget,
    deploymentProfile,
    signablePath: absoluteArtifactPath,
    signaturePath: `${absoluteArtifactPath}.sig`,
    publicKeyPath: `${absoluteArtifactPath}.pub.pem`,
    sbomPath: `${absoluteArtifactPath}.cdx.json`,
    provenancePath: `${absoluteArtifactPath}.intoto.jsonl`,
  };
}

function opensslArgs(command, keyPath, outputPath, inputPath, password) {
  if (command === 'public-key') {
    return ['pkey', '-in', keyPath, ...(password ? ['-passin', 'env:SDKWORK_RELEASE_SIGNING_PRIVATE_KEY_PASSWORD'] : []), '-pubout', '-out', outputPath];
  }
  if (command === 'sign') {
    return ['dgst', '-sha256', '-sign', keyPath, ...(password ? ['-passin', 'env:SDKWORK_RELEASE_SIGNING_PRIVATE_KEY_PASSWORD'] : []), '-out', outputPath, inputPath];
  }
  return ['dgst', '-sha256', '-verify', keyPath, '-signature', outputPath, inputPath];
}

export async function signArtifact(binding, env = process.env) {
  const privateKey = requiredEnv('SDKWORK_RELEASE_SIGNING_PRIVATE_KEY', env);
  const password = String(env.SDKWORK_RELEASE_SIGNING_PRIVATE_KEY_PASSWORD ?? '');
  if (binding.artifactKind === 'oci-image') {
    await writeFile(binding.signablePath, `${binding.locator}\n`, 'utf8');
  }
  await mkdir(path.dirname(binding.signaturePath), { recursive: true });
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), 'sdkwork-release-signing-'));
  const privateKeyPath = path.join(temporaryRoot, 'release-key.pem');
  const cleanupOnSignal = (exitCode) => {
    rmSync(temporaryRoot, { recursive: true, force: true });
    process.exit(exitCode);
  };
  const onSigint = () => cleanupOnSignal(130);
  const onSigterm = () => cleanupOnSignal(143);
  process.once('SIGINT', onSigint);
  process.once('SIGTERM', onSigterm);
  try {
    await writeFile(privateKeyPath, privateKey, { encoding: 'utf8', mode: 0o600 });
    await chmod(privateKeyPath, 0o600);
    const opensslEnv = { ...process.env, ...env };
    run('openssl', opensslArgs('public-key', privateKeyPath, binding.publicKeyPath, null, password), { env: opensslEnv });
    run('openssl', opensslArgs('sign', privateKeyPath, binding.signaturePath, binding.signablePath, password), { env: opensslEnv });
  } finally {
    process.removeListener('SIGINT', onSigint);
    process.removeListener('SIGTERM', onSigterm);
    await rm(temporaryRoot, { recursive: true, force: true });
  }
  return binding.signaturePath;
}

export function verifyArtifactSignature(binding) {
  for (const filePath of [binding.signablePath, binding.signaturePath, binding.publicKeyPath]) {
    if (!existsSync(filePath)) throw new Error(`missing signing evidence: ${toRelative(filePath)}`);
  }
  run('openssl', opensslArgs('verify', binding.publicKeyPath, binding.signaturePath, binding.signablePath));
}

let cachedCargoMetadata;

function cargoMetadata() {
  cachedCargoMetadata ??= JSON.parse(run('cargo', [
    'metadata',
    '--format-version', '1',
    '--locked',
    '--no-default-features',
    '--features', 'platform-foundation-suite',
  ]));
  return cachedCargoMetadata;
}

function bomReference(pkg) {
  const source = pkg.source ? `?repository_url=${encodeURIComponent(pkg.source)}` : '';
  return `pkg:cargo/${encodeURIComponent(pkg.name)}@${encodeURIComponent(pkg.version)}${source}`;
}

function deterministicUuid(seed) {
  const value = createHash('sha256').update(seed).digest('hex').slice(0, 32).split('');
  value[12] = '4';
  value[16] = ((Number.parseInt(value[16], 16) & 0x3) | 0x8).toString(16);
  const hex = value.join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function cargoComponents(metadata) {
  return metadata.packages.map((pkg) => ({
    type: 'library',
    'bom-ref': bomReference(pkg),
    name: pkg.name,
    version: pkg.version,
    purl: bomReference(pkg),
    ...(pkg.license ? { licenses: [{ expression: pkg.license }] } : {}),
    properties: [
      { name: 'sdkwork:cargo:manifest-path', value: toRelative(pkg.manifest_path) },
      ...(pkg.source ? [{ name: 'sdkwork:cargo:source', value: pkg.source }] : []),
    ],
  })).sort((left, right) => left['bom-ref'].localeCompare(right['bom-ref']));
}

function cargoDependencies(metadata) {
  const byId = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  return (metadata.resolve?.nodes ?? []).flatMap((node) => {
    const pkg = byId.get(node.id);
    if (!pkg) return [];
    return [{
      ref: bomReference(pkg),
      dependsOn: [...new Set((node.deps ?? []).map((dep) => byId.get(dep.pkg)).filter(Boolean).map(bomReference))].sort(),
    }];
  }).sort((left, right) => left.ref.localeCompare(right.ref));
}

export async function writeSbom(binding) {
  const metadata = cargoMetadata();
  const sourceCommit = run('git', ['rev-parse', 'HEAD']);
  const bom = {
    bomFormat: 'CycloneDX',
    specVersion: '1.6',
    serialNumber: `urn:uuid:${deterministicUuid(`${binding.packageId}:${binding.digest}`)}`,
    version: 1,
    metadata: {
      timestamp: new Date().toISOString(),
      tools: { components: [{ type: 'application', name: 'sdkwork-release-evidence', version: '1' }] },
      component: {
        type: binding.artifactKind === 'oci-image' ? 'container' : 'application',
        'bom-ref': `pkg:generic/sdkwork-api-cloud-gateway@${binding.version}?package_id=${encodeURIComponent(binding.packageId)}`,
        name: 'sdkwork-api-cloud-gateway',
        version: binding.version,
        hashes: [{ alg: 'SHA-256', content: SHA256_PATTERN.exec(binding.digest)[1] }],
        properties: [
          { name: 'sdkwork:package-id', value: binding.packageId },
          { name: 'sdkwork:deployment-profile', value: binding.deploymentProfile },
          { name: 'sdkwork:runtime-target', value: binding.runtimeTarget },
          { name: 'sdkwork:source-commit', value: sourceCommit },
          { name: 'sdkwork:artifact-locator', value: binding.locator },
        ],
      },
    },
    components: cargoComponents(metadata),
    dependencies: cargoDependencies(metadata),
  };
  await writeFile(binding.sbomPath, `${JSON.stringify(bom, null, 2)}\n`, 'utf8');
  return binding.sbomPath;
}

function gitResolvedDependencies(metadata) {
  const roots = new Set(metadata.packages.map((pkg) => findGitRoot(path.dirname(pkg.manifest_path))).filter(Boolean));
  const repositories = new Map();
  for (const root of roots) {
    if (repositories.has(root)) continue;
    const commit = run('git', ['rev-parse', 'HEAD'], { cwd: root });
    let remote = `file://${root.split(path.sep).join('/')}`;
    try {
      remote = run('git', ['remote', 'get-url', 'origin'], { cwd: root }) || remote;
    } catch {
      // A local-only dependency is still identified by its absolute repository root and commit.
    }
    repositories.set(root, { uri: remote, digest: { gitCommit: commit } });
  }
  const lockedCargoPackages = metadata.packages
    .filter((pkg) => pkg.source)
    .map((pkg) => ({ uri: bomReference(pkg) }));
  return [...repositories.values(), ...lockedCargoPackages]
    .sort((left, right) => left.uri.localeCompare(right.uri));
}

function findGitRoot(start) {
  let current = path.resolve(start);
  for (;;) {
    if (existsSync(path.join(current, '.git'))) return current;
    const parent = path.dirname(current);
    if (parent === current) return null;
    current = parent;
  }
}

export async function writeProvenance(binding) {
  const metadata = cargoMetadata();
  const sourceCommit = run('git', ['rev-parse', 'HEAD']);
  const dirty = run('git', ['status', '--porcelain']).length > 0;
  const statement = {
    _type: 'https://in-toto.io/Statement/v1',
    subject: [{
      name: binding.locator,
      digest: { sha256: SHA256_PATTERN.exec(binding.digest)[1] },
    }],
    predicateType: 'https://slsa.dev/provenance/v1',
    predicate: {
      buildDefinition: {
        buildType: 'https://sdkwork.com/buildtypes/github-workflow/v1',
        externalParameters: {
          appId: 'sdkwork-api-cloud-gateway',
          packageId: binding.packageId,
          version: binding.version,
          deploymentProfile: binding.deploymentProfile,
          runtimeTarget: binding.runtimeTarget,
          artifactKind: binding.artifactKind,
        },
        internalParameters: {
          workflow: 'sdkwork.workflow.json',
          cargoLocked: true,
          sourceTreeDirty: dirty,
        },
        resolvedDependencies: gitResolvedDependencies(metadata),
      },
      runDetails: {
        builder: { id: process.env.GITHUB_WORKFLOW_REF || 'https://sdkwork.com/builders/local-release-evidence/v1' },
        metadata: {
          invocationId: process.env.GITHUB_RUN_ID || `local:${sourceCommit}`,
          startedOn: new Date().toISOString(),
          finishedOn: new Date().toISOString(),
        },
      },
    },
  };
  await writeFile(binding.provenancePath, `${JSON.stringify(statement)}\n`, 'utf8');
  return binding.provenancePath;
}

function evidencePaths(env = process.env) {
  const configured = String(env.SDKWORK_ARTIFACT_EVIDENCE_PATHS || env.SDKWORK_ARTIFACT_EVIDENCE_PATH || '').trim();
  return (configured || `.sdkwork/evidence/${requiredEnv('SDKWORK_PACKAGE_ID', env)}.json`)
    .split(/\r?\n/u)
    .map((value) => safeRelativePath(value.trim(), 'artifact evidence path'));
}

export async function writeArtifactEvidence(binding, env = process.env) {
  await writeProvenance(binding);
  verifyArtifactSignature(binding);
  if (!existsSync(binding.sbomPath)) throw new Error(`missing SBOM: ${toRelative(binding.sbomPath)}`);
  const workflowCli = env.SDKWORK_WORKFLOW_CLI
    || path.resolve(appRoot, '..', 'sdkwork-github-workflow', 'scripts', 'sdkwork-workflow.mjs');
  if (!existsSync(workflowCli)) throw new Error(`SDKWork workflow CLI does not exist: ${workflowCli}`);
  let evidence;
  for (const outputPath of evidencePaths(env)) {
    run('node', [
      workflowCli,
      'evidence:create',
      '--config', 'sdkwork.workflow.json',
      '--target-id', requiredEnv('SDKWORK_PACKAGE_TARGET_ID', env),
      '--deployment-profile', binding.deploymentProfile,
      '--runtime-target', binding.runtimeTarget,
      '--version', binding.version,
      '--artifact', binding.artifactPath,
      '--artifact-kind', binding.artifactKind,
      ...(binding.artifactKind === 'oci-image' ? ['--artifact-locator', binding.locator] : []),
      '--artifact-evidence', outputPath,
      '--sbom', toRelative(binding.sbomPath),
      '--provenance', toRelative(binding.provenancePath),
      '--signature', toRelative(binding.signaturePath),
    ], { env: { ...process.env, ...env } });
    evidence = JSON.parse(await readFile(path.resolve(appRoot, outputPath), 'utf8'));
  }
  return evidence;
}

export async function verifyReleaseEvidence(binding, env = process.env) {
  verifyArtifactSignature(binding);
  for (const filePath of [binding.sbomPath, binding.provenancePath]) {
    if (!existsSync(filePath)) throw new Error(`missing release evidence: ${toRelative(filePath)}`);
    JSON.parse((await readFile(filePath, 'utf8')).trim());
  }
  for (const evidencePath of evidencePaths(env)) {
    const evidence = JSON.parse(await readFile(path.resolve(appRoot, evidencePath), 'utf8'));
    if (evidence.digest !== binding.digest) throw new Error(`artifact evidence digest mismatch: ${evidencePath}`);
    if (evidence.artifactKind !== binding.artifactKind) throw new Error(`artifact evidence kind mismatch: ${evidencePath}`);
    if (evidence.packageId !== binding.packageId) throw new Error(`artifact evidence package mismatch: ${evidencePath}`);
  }
}

async function main(argv = process.argv.slice(2), env = process.env) {
  const command = argv[0] ?? 'help';
  if (command === 'help' || command === '--help' || command === '-h') {
    printHelp();
    return;
  }
  const binding = await resolveArtifactBinding(env);
  if (command === 'sign') await signArtifact(binding, env);
  else if (command === 'sbom') await writeSbom(binding);
  else if (command === 'evidence') await writeArtifactEvidence(binding, env);
  else if (command === 'verify') await verifyReleaseEvidence(binding, env);
  else throw new Error(`unknown command: ${command}`);
  console.log(`[release-evidence] ${command} complete for ${binding.packageId} (${binding.digest})`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`[release-evidence] ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  });
}

export { main };

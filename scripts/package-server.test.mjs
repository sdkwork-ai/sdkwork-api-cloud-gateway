import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  DATABASE_ASSET_METADATA_ARGS,
  RELEASE_CONFIG_TEMPLATES,
  createCargoBuildCommands,
  resolveCargoReleaseDir,
  resolveCargoTargetDir,
  resolveDatabaseHostCratesFromMetadata,
  resolveDeploymentProfile,
  resolveExecutableName,
  resolveServerStageName,
  resolveTarArchiveName,
  validateArchiveEntryPaths,
  verifyArtifactChecksum,
  verifyStageChecksums,
  writeArtifactChecksum,
} from './package-server.mjs';
import {
  resolveContainerPackageId,
  validateImageManifestRecord,
} from './build-api-cloud-gateway-container.mjs';
import {
  resolveArtifactBinding,
  signArtifact,
  verifyArtifactSignature,
  verifyReleaseEvidence,
  writeArtifactEvidence,
  writeSbom,
} from './release-evidence.mjs';

const root = path.resolve('workspace', 'sdkwork-api-cloud-gateway');

test('cargo release directory defaults to the application target', () => {
  assert.equal(resolveCargoTargetDir(root, undefined), path.join(root, 'target'));
  assert.equal(
    resolveCargoReleaseDir(root, undefined),
    path.join(root, 'target', 'release'),
  );
});

test('cargo release directory honors absolute and relative target overrides', () => {
  const absoluteTarget = path.resolve('cargo-targets', 'gateway');
  assert.equal(resolveCargoTargetDir(root, absoluteTarget), absoluteTarget);
  assert.equal(
    resolveCargoTargetDir(root, '.cache/cargo-target'),
    path.join(root, '.cache', 'cargo-target'),
  );
  assert.equal(
    resolveCargoReleaseDir(root, absoluteTarget),
    path.join(absoluteTarget, 'release'),
  );
  assert.equal(
    resolveCargoReleaseDir(root, '.cache/cargo-target'),
    path.join(root, '.cache', 'cargo-target', 'release'),
  );
});

test('cargo release directory includes an explicit Rust target triple', () => {
  assert.equal(
    resolveCargoReleaseDir(root, 'target-custom', 'x86_64-unknown-linux-gnu'),
    path.join(root, 'target-custom', 'x86_64-unknown-linux-gnu', 'release'),
  );
});

test('all packaged executables use the target platform suffix', () => {
  assert.equal(
    resolveExecutableName('sdkwork-api-cloud-gateway', 'windows'),
    'sdkwork-api-cloud-gateway.exe',
  );
  assert.equal(
    resolveExecutableName('sdkwork-intelligence-knowledgebase-rpc-bin', 'windows'),
    'sdkwork-intelligence-knowledgebase-rpc-bin.exe',
  );
  assert.equal(
    resolveExecutableName('sdkwork-intelligence-knowledgebase-rpc-bin', 'linux'),
    'sdkwork-intelligence-knowledgebase-rpc-bin',
  );
});

test('release builds share lockfile and Rust target requirements', () => {
  const commands = createCargoBuildCommands({
    rustTarget: 'x86_64-unknown-linux-gnu',
  });

  assert.equal(commands.length, 2);
  for (const command of commands) {
    assert.ok(command.args.includes('--locked'));
    assert.deepEqual(
      command.args.slice(command.args.indexOf('--target')),
      ['--target', 'x86_64-unknown-linux-gnu'],
    );
  }
});

test('server stage names isolate standalone and cloud artifacts', () => {
  const input = {
    architecture: 'x64',
    platform: 'linux',
    version: '0.1.0',
  };
  assert.equal(
    resolveServerStageName({ ...input, deploymentProfile: 'standalone' }),
    'sdkwork-api-cloud-gateway-0.1.0-linux-x64-standalone-server',
  );
  assert.equal(
    resolveServerStageName({ ...input, deploymentProfile: 'cloud' }),
    'sdkwork-api-cloud-gateway-0.1.0-linux-x64-cloud-server',
  );
});

test('tar receives a local archive name instead of a Windows drive path', () => {
  const distRoot = path.join(root, 'dist', 'server');
  const archivePath = path.join(distRoot, 'gateway.tar.gz');

  assert.equal(resolveTarArchiveName(distRoot, archivePath), 'gateway.tar.gz');
  assert.throws(
    () => resolveTarArchiveName(distRoot, path.join(root, 'outside.tar.gz')),
    /Archive path must stay inside/,
  );
});

test('archive entry validation rejects paths outside the canonical stage root', () => {
  assert.deepEqual(
    [...validateArchiveEntryPaths('release/\nrelease/bin/gateway\n', 'release')],
    ['release', 'release/bin/gateway'],
  );
  assert.throws(
    () => validateArchiveEntryPaths('release/../outside\n', 'release'),
    /unsafe entry/u,
  );
  assert.throws(
    () => validateArchiveEntryPaths('other/bin/gateway\n', 'release'),
    /unsafe entry/u,
  );
});

test('stage checksum verification fails closed on tampering and unlisted files', async (t) => {
  const stageRoot = await mkdtemp(path.join(tmpdir(), 'sdkwork-gateway-checksums-'));
  t.after(async () => rm(stageRoot, { recursive: true, force: true }));
  await mkdir(path.join(stageRoot, 'bin'));
  const binaryPath = path.join(stageRoot, 'bin', 'gateway');
  await writeFile(binaryPath, 'release-binary', 'utf8');
  const digest = createHash('sha256').update('release-binary').digest('hex');
  await writeFile(
    path.join(stageRoot, 'checksums.sha256'),
    `${digest}  bin/gateway\n`,
    'utf8',
  );

  await assert.doesNotReject(verifyStageChecksums(stageRoot));
  await writeFile(binaryPath, 'tampered-binary', 'utf8');
  await assert.rejects(verifyStageChecksums(stageRoot), /Checksum mismatch/u);
  await writeFile(binaryPath, 'release-binary', 'utf8');
  await writeFile(path.join(stageRoot, 'unlisted.txt'), 'unexpected', 'utf8');
  await assert.rejects(verifyStageChecksums(stageRoot), /is missing unlisted\.txt/u);
});

test('artifact checksum sidecar binds the immutable archive bytes', async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), 'sdkwork-gateway-artifact-'));
  t.after(async () => rm(root, { recursive: true, force: true }));
  const artifactPath = path.join(root, 'gateway.tar.gz');
  await writeFile(artifactPath, 'archive-v1', 'utf8');

  const digest = await writeArtifactChecksum(artifactPath);
  assert.equal(digest, createHash('sha256').update('archive-v1').digest('hex'));
  assert.equal(await verifyArtifactChecksum(artifactPath), digest);
  await writeFile(artifactPath, 'archive-v2', 'utf8');
  await assert.rejects(verifyArtifactChecksum(artifactPath), /checksum mismatch/iu);
});

test('deployment profile follows workflow standard with legacy compatibility', () => {
  assert.equal(resolveDeploymentProfile(), 'standalone');
  assert.equal(
    resolveDeploymentProfile({ legacyEnvironment: 'cloud' }),
    'cloud',
  );
  assert.equal(
    resolveDeploymentProfile({
      legacyEnvironment: 'cloud',
      standardEnvironment: 'standalone',
    }),
    'standalone',
  );
  assert.equal(
    resolveDeploymentProfile({
      explicit: 'cloud',
      legacyEnvironment: 'standalone',
      standardEnvironment: 'standalone',
    }),
    'cloud',
  );
});

test('database assets are discovered through the owner assembly dependency closure', () => {
  const gatewayManifest = path.join(
    path.resolve('.'),
    'crates',
    'sdkwork-api-cloud-gateway',
    'Cargo.toml',
  );
  const metadata = {
    packages: [
      {
        id: 'gateway',
        name: 'sdkwork-api-cloud-gateway',
        manifest_path: gatewayManifest,
        dependencies: [
          {
            name: 'sdkwork-api-audio-assembly',
            path: path.resolve('..', 'sdkwork-audio', 'crates', 'sdkwork-api-audio-assembly'),
          },
          { name: 'serde', path: null },
        ],
      },
      {
        id: 'assembly',
        name: 'sdkwork-api-audio-assembly',
        manifest_path: path.resolve('..', 'sdkwork-audio', 'crates', 'sdkwork-api-audio-assembly', 'Cargo.toml'),
        dependencies: [],
        source: null,
      },
      {
        id: 'database-host',
        name: 'sdkwork-audio-database-host',
        manifest_path: path.resolve('..', 'sdkwork-audio', 'crates', 'sdkwork-audio-database-host', 'Cargo.toml'),
        dependencies: [],
        source: null,
      },
    ],
    resolve: {
      nodes: [
        { id: 'gateway', deps: [{ pkg: 'assembly', dep_kinds: [{ kind: null }] }] },
        { id: 'assembly', deps: [{ pkg: 'database-host', dep_kinds: [{ kind: null }] }] },
        { id: 'database-host', deps: [] },
      ],
    },
  };

  assert.deepEqual(resolveDatabaseHostCratesFromMetadata(metadata), [
    {
      name: 'sdkwork-audio-database-host',
      crateRoot: path.resolve(
        '..',
        'sdkwork-audio',
        'crates',
        'sdkwork-audio-database-host',
      ),
    },
  ]);
});

test('database asset metadata selects the production assembly feature closure', () => {
  assert.deepEqual(DATABASE_ASSET_METADATA_ARGS, [
    'metadata',
    '--format-version',
    '1',
    '--locked',
    '--no-default-features',
    '--features',
    'platform-foundation-suite',
  ]);
});

test('database asset discovery rejects direct gateway database hosts', () => {
  assert.throws(
    () => resolveDatabaseHostCratesFromMetadata({
      packages: [{
        id: 'gateway',
        name: 'sdkwork-api-cloud-gateway',
        manifest_path: path.join(
          path.resolve('.'),
          'crates',
          'sdkwork-api-cloud-gateway',
          'Cargo.toml',
        ),
        dependencies: [{
          name: 'sdkwork-audio-database-host',
          path: path.resolve('..', 'sdkwork-audio', 'crates', 'sdkwork-audio-database-host'),
        }],
      }],
      resolve: { nodes: [] },
    }),
    /must consume database lifecycle through owner API assemblies/u,
  );
});

test('container image identity uses the canonical workflow package id', () => {
  assert.equal(
    resolveContainerPackageId(),
    'container-x64-standalone-container-oci',
  );
  assert.equal(
    resolveContainerPackageId('cloud', 'arm64'),
    'container-arm64-cloud-container-oci',
  );
});

test('release packages carry only the production-safe runtime config template', () => {
  assert.deepEqual(RELEASE_CONFIG_TEMPLATES, [
    'sdkwork-api-cloud-gateway.production.toml',
  ]);

  const containerPackager = readFileSync(
    path.resolve('.', 'scripts', 'build-api-cloud-gateway-container.mjs'),
    'utf8',
  );
  assert.match(containerPackager, /RELEASE_CONFIG_TEMPLATES/u);
  assert.doesNotMatch(containerPackager, /development\.toml|staging\.toml|test\.toml/u);
});

test('container image manifest validation rejects package and digest drift', () => {
  const valid = {
    schemaVersion: '2026-08-08.container-image.v1',
    packageId: 'container-x64-standalone-container-oci',
    version: '0.1.0',
    repoDigest: `registry.sdkwork.com/gateway@sha256:${'a'.repeat(64)}`,
    imageId: `sha256:${'b'.repeat(64)}`,
    sizeBytes: 1024,
  };
  assert.deepEqual(validateImageManifestRecord(valid, { version: '0.1.0' }), []);
  assert.ok(
    validateImageManifestRecord(
      { ...valid, packageId: 'linux-x64-standalone-server-container' },
      { version: '0.1.0' },
    ).some((issue) => issue.includes('packageId')),
  );
  assert.ok(
    validateImageManifestRecord(
      { ...valid, repoDigest: null },
      { version: '0.1.0', requireRepoDigest: true },
    ).some((issue) => issue.includes('repoDigest is required')),
  );
});

test('release evidence signs server bytes and requires a remote OCI digest', async (t) => {
  if (spawnSync('openssl', ['version'], { encoding: 'utf8' }).status !== 0) {
    t.skip('openssl is not installed');
    return;
  }
  const releaseRoot = path.resolve('.tmp', 'release-evidence-test');
  t.after(async () => rm(releaseRoot, { recursive: true, force: true }));
  await mkdir(releaseRoot, { recursive: true });
  const artifactPath = path.join(releaseRoot, 'gateway.tar.gz');
  const privateKeyPath = path.join(releaseRoot, 'private.pem');
  await writeFile(artifactPath, 'signed-server-archive', 'utf8');
  const generated = spawnSync('openssl', [
    'genpkey', '-algorithm', 'RSA', '-pkeyopt', 'rsa_keygen_bits:2048', '-out', privateKeyPath,
  ], { encoding: 'utf8' });
  assert.equal(generated.status, 0, generated.stderr);
  const privateKey = readFileSync(privateKeyPath, 'utf8');
  const baseEnv = {
    SDKWORK_PACKAGE_ID: 'linux-x64-standalone-server-tar-gz',
    SDKWORK_PACKAGE_VERSION: '0.1.0',
    SDKWORK_DEPLOYMENT_PROFILE: 'standalone',
    SDKWORK_RUNTIME_TARGET: 'server',
    SDKWORK_PACKAGE_ARTIFACT_PATH: path.relative(path.resolve('.'), artifactPath).split(path.sep).join('/'),
    SDKWORK_RELEASE_SIGNING_PRIVATE_KEY: privateKey,
    SDKWORK_PACKAGE_TARGET_ID: 'linux-x64-standalone-server-tar-gz',
    SDKWORK_ARTIFACT_EVIDENCE_PATHS: path.relative(
      path.resolve('.'),
      path.join(releaseRoot, 'artifact-evidence.json'),
    ).split(path.sep).join('/'),
  };
  const server = await resolveArtifactBinding(baseEnv);
  await signArtifact(server, baseEnv);
  assert.doesNotThrow(() => verifyArtifactSignature(server));
  await writeSbom(server);
  const evidence = await writeArtifactEvidence(server, baseEnv);
  await verifyReleaseEvidence(server, baseEnv);
  assert.equal(evidence.artifactKind, 'file');
  assert.equal(evidence.digest, server.digest);
  assert.equal(JSON.parse(readFileSync(server.sbomPath, 'utf8')).bomFormat, 'CycloneDX');
  assert.equal(
    JSON.parse(readFileSync(server.provenancePath, 'utf8')).predicateType,
    'https://slsa.dev/provenance/v1',
  );

  const containerManifestPath = path.join(releaseRoot, 'container-image.json');
  const containerEnv = {
    ...baseEnv,
    SDKWORK_PACKAGE_ID: 'container-x64-standalone-container-oci',
    SDKWORK_RUNTIME_TARGET: 'container',
    SDKWORK_PACKAGE_ARTIFACT_PATH: path.relative(path.resolve('.'), containerManifestPath).split(path.sep).join('/'),
  };
  await writeFile(containerManifestPath, JSON.stringify({
    packageId: containerEnv.SDKWORK_PACKAGE_ID,
    version: '0.1.0',
    repoDigest: null,
  }), 'utf8');
  await assert.rejects(() => resolveArtifactBinding(containerEnv), /pushed repository@sha256 digest/u);
  const repoDigest = `registry.sdkwork.com/gateway@sha256:${'e'.repeat(64)}`;
  await writeFile(containerManifestPath, JSON.stringify({
    packageId: containerEnv.SDKWORK_PACKAGE_ID,
    version: '0.1.0',
    repoDigest,
  }), 'utf8');
  const container = await resolveArtifactBinding(containerEnv);
  assert.equal(container.digest, `sha256:${'e'.repeat(64)}`);
  assert.equal(container.locator, repoDigest);
});

test('server package source declares the official Knowledgebase RPC sidecar', () => {
  const packager = readFileSync(
    path.resolve('.', 'scripts', 'package-server.mjs'),
    'utf8',
  );
  assert.match(packager, /sdkwork-intelligence-knowledgebase-rpc-bin/u);
  assert.match(packager, /sdkwork-knowledgebase-rpc\.service/u);
  assert.match(packager, /knowledgebase-rpc-entrypoint/u);
});

test('workflow package matrix publishes only targets that can carry release evidence', () => {
  const workflow = JSON.parse(readFileSync(path.resolve('.', 'agent.workflow.json'), 'utf8'));
  // OCI release evidence must bind to a pushed repository@sha256 digest and
  // SUPPLY_CHAIN_SECURITY_SPEC forbids substituting a local image id. The
  // GitHub workflow has no registry credentials, so container images stay on
  // the bin/docker-image.sh channel and must not appear in the release matrix.
  const containerTargets = workflow.targets.filter((target) => target.runtimeTarget === 'container');
  assert.deepEqual(containerTargets, []);
  for (const target of workflow.targets) {
    assert.equal(target.runtimeTarget, 'server');
    assert.equal(target.profile, 'server');
    assert.ok(['linux', 'windows', 'macos'].includes(target.platform));
  }
  assert.throws(() => resolveContainerPackageId('unknown-profile'), /Unsupported container deployment profile/u);
});

import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';

import {
  resolveCargoReleaseDir,
  resolveDatabaseHostCratesFromMetadata,
  resolveDeploymentProfile,
  resolveServerStageName,
  resolveTarArchiveName,
} from './package-server.mjs';
import { resolveMissingDatabaseWorkspaces } from './build-api-cloud-gateway-container.mjs';

const root = path.resolve('workspace', 'sdkwork-api-cloud-gateway');

test('cargo release directory defaults to the application target', () => {
  assert.equal(
    resolveCargoReleaseDir(root, undefined),
    path.join(root, 'target', 'release'),
  );
});

test('cargo release directory honors absolute and relative target overrides', () => {
  const absoluteTarget = path.resolve('cargo-targets', 'gateway');
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

test('database assets are discovered from convention-named gateway host dependencies', () => {
  const gatewayManifest = path.join(
    path.resolve('.'),
    'crates',
    'sdkwork-api-cloud-gateway',
    'Cargo.toml',
  );
  const metadata = {
    packages: [
      {
        name: 'sdkwork-api-cloud-gateway',
        manifest_path: gatewayManifest,
        dependencies: [
          {
            name: 'sdkwork-audio-database-host',
            path: path.resolve('..', 'sdkwork-audio', 'crates', 'sdkwork-audio-database-host'),
          },
          {
            name: 'sdkwork-api-audio-assembly',
            path: path.resolve('..', 'sdkwork-audio', 'crates', 'sdkwork-api-audio-assembly'),
          },
          { name: 'serde', path: null },
        ],
      },
    ],
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

test('container database extras exclude workspaces already discovered from Cargo', () => {
  assert.deepEqual(
    resolveMissingDatabaseWorkspaces(
      [{ workspace: 'sdkwork-models' }, { workspace: 'sdkwork-cloudrouter' }],
      ['sdkwork-log', 'sdkwork-models', 'sdkwork-partner'],
    ),
    ['sdkwork-log', 'sdkwork-partner'],
  );
});

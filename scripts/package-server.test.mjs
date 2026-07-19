import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';

import {
  resolveCargoReleaseDir,
  resolveDeploymentProfile,
  resolveServerStageName,
} from './package-server.mjs';

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

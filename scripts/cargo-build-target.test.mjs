/**
 * Contract tests for the Cargo target-directory resolver.
 *
 * The resolver decides where a Linux release build writes `target/`. Getting it
 * wrong is expensive in both directions: relocating on a native filesystem
 * throws away an existing cache, and refusing to relocate on a Windows
 * passthrough mount is what made packaging take tens of minutes.
 */

import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';

import {
  WINDOWS_PASSTHROUGH_FILESYSTEM_TYPES,
  describeCargoBuildTargetDir,
  isWindowsPassthroughFilesystem,
  probeFilesystemType,
  resolveCacheRoot,
  resolveCargoBuildTargetDir,
} from './cargo-build-target.mjs';

const appRoot = path.resolve(path.sep, 'workspace', 'sdkwork-api-cloud-gateway');
const appId = 'sdkwork-api-cloud-gateway';
const homeDir = path.resolve(path.sep, 'home', 'builder');

function resolve(options = {}) {
  return resolveCargoBuildTargetDir({
    appRoot,
    appId,
    env: {},
    platform: 'linux',
    filesystemType: 'v9fs',
    homeDir,
    ...options,
  });
}

test('an explicit CARGO_TARGET_DIR always wins', () => {
  const absolute = path.resolve(path.sep, 'cargo-targets', 'gateway');
  assert.deepEqual(
    resolve({ env: { CARGO_TARGET_DIR: absolute }, filesystemType: 'v9fs' }),
    { targetDir: absolute, source: 'explicit', filesystemType: '' },
  );

  assert.equal(
    resolve({ env: { CARGO_TARGET_DIR: '.cache/cargo-target' } }).targetDir,
    path.join(appRoot, '.cache', 'cargo-target'),
  );

  // The override is platform independent: Windows packaging must keep honouring it.
  assert.equal(
    resolve({ env: { CARGO_TARGET_DIR: 'custom' }, platform: 'win32' }).source,
    'explicit',
  );
});

test('a Windows passthrough mount relocates the target directory into the guest cache', () => {
  const resolved = resolve({ env: { XDG_CACHE_HOME: path.resolve(path.sep, 'cache') } });
  assert.equal(resolved.source, 'relocated-posix');
  assert.equal(resolved.filesystemType, 'v9fs');
  assert.equal(
    resolved.targetDir,
    path.join(path.resolve(path.sep, 'cache'), 'sdkwork', 'cargo-target', appId),
  );
});

test('the relocation falls back to ~/.cache when XDG_CACHE_HOME is unset', () => {
  assert.equal(
    resolve({ env: {} }).targetDir,
    path.join(homeDir, '.cache', 'sdkwork', 'cargo-target', appId),
  );
});

test('a relative XDG_CACHE_HOME is ignored per the XDG base directory spec', () => {
  assert.equal(
    resolve({ env: { XDG_CACHE_HOME: 'relative-cache' } }).targetDir,
    path.join(homeDir, '.cache', 'sdkwork', 'cargo-target', appId),
  );
});

test('native and non-Linux hosts keep the repository target directory', () => {
  for (const filesystemType of ['ext2/ext3', 'overlay', 'xfs', 'btrfs', 'tmpfs', '', undefined]) {
    const resolved = resolve({ filesystemType });
    assert.equal(resolved.source, 'default', `filesystem ${String(filesystemType)}`);
    assert.equal(resolved.targetDir, path.join(appRoot, 'target'));
  }

  assert.equal(resolve({ platform: 'win32', filesystemType: 'v9fs' }).targetDir, path.join(appRoot, 'target'));
  assert.equal(resolve({ platform: 'darwin', filesystemType: 'v9fs' }).targetDir, path.join(appRoot, 'target'));
});

test('an unknown filesystem type fails closed to the historical layout', () => {
  assert.equal(probeFilesystemType(''), '');
  const failedProbe = probeFilesystemType(appRoot, () => ({ error: new Error('spawn stat ENOENT') }));
  assert.equal(failedProbe, '');
  assert.equal(resolve({ filesystemType: failedProbe }).source, 'default');
});

test('filesystem types are classified by the shared passthrough list', () => {
  for (const filesystemType of WINDOWS_PASSTHROUGH_FILESYSTEM_TYPES) {
    assert.equal(isWindowsPassthroughFilesystem(filesystemType), true, filesystemType);
    assert.equal(isWindowsPassthroughFilesystem(filesystemType.toUpperCase()), true, filesystemType);
  }
  for (const filesystemType of ['ext2/ext3', 'overlay', 'xfs', 'btrfs', 'zfs', 'tmpfs', '']) {
    assert.equal(isWindowsPassthroughFilesystem(filesystemType), false, filesystemType);
  }
  assert.equal(isWindowsPassthroughFilesystem(undefined), false);
  assert.equal(isWindowsPassthroughFilesystem(null), false);
});

test('probeFilesystemType reports the stat result and swallows probe failures', () => {
  const calls = [];
  const spawn = (command, args) => {
    calls.push([command, ...args]);
    return { status: 0, stdout: 'v9fs\n' };
  };
  assert.equal(probeFilesystemType(appRoot, spawn), 'v9fs');
  assert.deepEqual(calls, [['stat', '-f', '-c', '%T', appRoot]]);

  assert.equal(
    probeFilesystemType(appRoot, () => ({ status: 1, stdout: '' })),
    '',
  );
});

test('resolveCacheRoot requires either an absolute XDG_CACHE_HOME or a home directory', () => {
  const absolute = path.resolve(path.sep, 'xdg-cache');
  assert.equal(resolveCacheRoot({ env: { XDG_CACHE_HOME: absolute }, homeDir }), absolute);
  assert.equal(resolveCacheRoot({ env: {}, homeDir }), path.join(homeDir, '.cache'));
  assert.throws(() => resolveCacheRoot({ env: {}, homeDir: '' }), /cache root/u);
});

test('the resolver rejects an incomplete request', () => {
  assert.throws(() => resolveCargoBuildTargetDir({ appId }), /appRoot/u);
  assert.throws(() => resolveCargoBuildTargetDir({ appRoot }), /appId/u);
});

test('the build log explains where the target directory came from', () => {
  assert.match(
    describeCargoBuildTargetDir({ targetDir: '/tmp/t', source: 'explicit', filesystemType: '' }),
    /CARGO_TARGET_DIR/u,
  );
  assert.match(
    describeCargoBuildTargetDir({ targetDir: '/tmp/t', source: 'relocated-posix', filesystemType: 'v9fs' }),
    /relocated off the v9fs mount/u,
  );
  assert.match(
    describeCargoBuildTargetDir({ targetDir: '/tmp/t', source: 'default', filesystemType: 'ext2/ext3' }),
    /^cargo target dir: \/tmp\/t$/u,
  );
});

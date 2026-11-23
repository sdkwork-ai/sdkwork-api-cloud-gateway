/**
 * Contract tests for the Linux release-build preflight.
 *
 * The probe exists to turn a multi-minute mid-graph failure in `audiopus_sys`
 * into an immediate, actionable message from the packaging entrypoint.
 */

import assert from 'node:assert/strict';
import test from 'node:test';

import {
  LINUX_BUILD_PREREQUISITE_HINT,
  OPUS_AUTOTOOLS_COMMANDS,
  hasAllCommands,
  hasPkgConfigModule,
  resolveLinuxBuildPrerequisiteIssues,
} from './linux-build-prerequisites.mjs';

/**
 * Fake process runner that answers by probing the argv it receives.
 *
 * @param {{ pkgConfigStatus?: number|null, presentCommands?: string[] }} answers
 * @returns {(command: string, args: string[]) => object}
 */
function fakeSpawn({ pkgConfigStatus = null, presentCommands = [] } = {}) {
  return (command, args) => {
    if (command === 'pkg-config') {
      if (pkgConfigStatus === null) {
        return { error: new Error('spawn pkg-config ENOENT'), status: null };
      }
      return { status: pkgConfigStatus };
    }
    if (command === 'sh') {
      const probe = String(args?.[1] ?? '');
      const hit = presentCommands.some((name) => probe.includes(`command -v ${name} `));
      return { status: hit ? 0 : 1 };
    }
    return { status: 1 };
  };
}

test('non-Linux hosts are never probed', () => {
  const spawn = fakeSpawn();
  for (const platform of ['win32', 'darwin']) {
    assert.deepEqual(
      resolveLinuxBuildPrerequisiteIssues({ platform, env: {}, spawn }),
      [],
      platform,
    );
  }
});

test('an explicit LIBOPUS_LIB_DIR short-circuits the probe', () => {
  assert.deepEqual(
    resolveLinuxBuildPrerequisiteIssues({
      platform: 'linux',
      env: { LIBOPUS_LIB_DIR: '/opt/opus/lib' },
      spawn: fakeSpawn(),
    }),
    [],
  );
});

test('host Opus through pkg-config satisfies the prerequisite', () => {
  assert.deepEqual(
    resolveLinuxBuildPrerequisiteIssues({
      platform: 'linux',
      env: {},
      spawn: fakeSpawn({ pkgConfigStatus: 0 }),
    }),
    [],
  );
});

test('the autotools fallback satisfies the prerequisite without host Opus', () => {
  assert.deepEqual(
    resolveLinuxBuildPrerequisiteIssues({
      platform: 'linux',
      env: {},
      spawn: fakeSpawn({ pkgConfigStatus: 1, presentCommands: [...OPUS_AUTOTOOLS_COMMANDS] }),
    }),
    [],
  );
});

test('a guest with neither path reports the exact remediation', () => {
  const issues = resolveLinuxBuildPrerequisiteIssues({
    platform: 'linux',
    env: {},
    spawn: fakeSpawn({ pkgConfigStatus: 1, presentCommands: [] }),
  });
  assert.equal(issues.length, 1);
  assert.match(issues[0], /pkg-config/u);
  assert.match(issues[0], /libopus-dev/u);
  assert.ok(issues[0].includes(LINUX_BUILD_PREREQUISITE_HINT));
});

test('a partially installed autotools chain still reports', () => {
  const issues = resolveLinuxBuildPrerequisiteIssues({
    platform: 'linux',
    env: {},
    spawn: fakeSpawn({ pkgConfigStatus: 1, presentCommands: ['autoreconf'] }),
  });
  assert.equal(issues.length, 1);
});

test('hasPkgConfigModule treats a missing binary and a non-zero status as absent', () => {
  assert.equal(hasPkgConfigModule('opus', fakeSpawn({ pkgConfigStatus: 0 })), true);
  assert.equal(hasPkgConfigModule('opus', fakeSpawn({ pkgConfigStatus: 1 })), false);
  assert.equal(hasPkgConfigModule('opus', fakeSpawn({ pkgConfigStatus: null })), false);
  assert.equal(hasPkgConfigModule('', fakeSpawn({ pkgConfigStatus: 0 })), false);
});

test('hasAllCommands requires every command to resolve', () => {
  assert.equal(hasAllCommands(['autoreconf', 'libtoolize'], fakeSpawn({
    presentCommands: ['autoreconf', 'libtoolize'],
  })), true);
  assert.equal(hasAllCommands(['autoreconf', 'libtoolize'], fakeSpawn({
    presentCommands: ['autoreconf'],
  })), false);
  assert.equal(hasAllCommands([], fakeSpawn()), true);
});

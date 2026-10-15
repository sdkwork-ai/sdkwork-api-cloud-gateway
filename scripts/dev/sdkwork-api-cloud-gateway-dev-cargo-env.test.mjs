import assert from 'node:assert/strict';
import test from 'node:test';

import { resolveDevelopmentCargoEnv, resolveDevelopmentOpusEnv } from './sdkwork-api-cloud-gateway-dev-cargo-env.mjs';

test('windows development builds disable incremental compilation', () => {
  assert.deepEqual(
    resolveDevelopmentCargoEnv({ env: {}, platform: 'win32' }),
    { CARGO_INCREMENTAL: '0' },
  );
});

test('development builds on other platforms leave the cargo default untouched', () => {
  assert.deepEqual(
    resolveDevelopmentCargoEnv({ env: {}, platform: 'linux' }),
    {},
  );
});

test('an explicit CARGO_INCREMENTAL wins over the development default', () => {
  assert.deepEqual(
    resolveDevelopmentCargoEnv({
      env: { CARGO_INCREMENTAL: '1' },
      platform: 'win32',
    }),
    {},
  );
  assert.deepEqual(
    resolveDevelopmentCargoEnv({
      env: { CARGO_INCREMENTAL: ' 0 ' },
      platform: 'win32',
    }),
    {},
  );
});

test('the runner opts back into incremental compilation through its own switch', () => {
  assert.deepEqual(
    resolveDevelopmentCargoEnv({
      env: { SDKWORK_API_CLOUD_GATEWAY_ALLOW_CARGO_INCREMENTAL: '1' },
      platform: 'win32',
    }),
    {},
  );
});

test('only the documented opt-in value re-enables incremental compilation', () => {
  assert.deepEqual(
    resolveDevelopmentCargoEnv({
      env: { SDKWORK_API_CLOUD_GATEWAY_ALLOW_CARGO_INCREMENTAL: 'true' },
      platform: 'win32',
    }),
    { CARGO_INCREMENTAL: '0' },
  );
});

test('unrelated cargo variables are never forwarded by the resolver', () => {
  assert.deepEqual(
    resolveDevelopmentCargoEnv({
      env: { CARGO_TARGET_DIR: 'D:\\cargo-target', RUSTFLAGS: '-Cdebuginfo=0' },
      platform: 'win32',
    }),
    { CARGO_INCREMENTAL: '0' },
  );
});

test('windows development links the prebuilt Opus library when it exists', () => {
  const resolved = resolveDevelopmentOpusEnv({
    appRoot: 'D:\\repo',
    env: {},
    fileExists: (filePath) => filePath === 'D:\\repo\\target\\prebuilt-opus\\lib\\libopus.a',
    platform: 'win32',
  });

  assert.equal(resolved.env.OPUS_LIB_DIR, 'D:\\repo\\target\\prebuilt-opus\\lib');
  assert.equal(resolved.env.OPUS_STATIC, '1');
  assert.equal(resolved.libraryDirectory, 'D:\\repo\\target\\prebuilt-opus\\lib');
});

test('a development machine without the prebuilt Opus library is left unconfigured', () => {
  // audiopus_sys fails with an opaque linker error when pointed at a directory
  // that holds no archive, so an absent library must not be configured.
  const resolved = resolveDevelopmentOpusEnv({
    appRoot: 'D:\\repo',
    env: {},
    fileExists: () => false,
    platform: 'win32',
  });

  assert.deepEqual(resolved.env, {});
  assert.equal(resolved.libraryDirectory, undefined);
});

test('an explicit Opus configuration wins over the prebuilt default', () => {
  const resolved = resolveDevelopmentOpusEnv({
    appRoot: 'D:\\repo',
    env: { OPUS_LIB_DIR: 'D:\\custom-opus' },
    fileExists: () => true,
    platform: 'win32',
  });

  assert.deepEqual(resolved.env, {});
  assert.equal(resolved.libraryDirectory, 'D:\\custom-opus');
  assert.deepEqual(
    resolveDevelopmentOpusEnv({
      appRoot: 'D:\\repo',
      env: { LIBOPUS_LIB_DIR: 'D:\\other-opus' },
      fileExists: () => true,
      platform: 'win32',
    }).env,
    {},
  );
});

test('non-windows development never rewrites the Opus link path', () => {
  const resolved = resolveDevelopmentOpusEnv({
    appRoot: '/repo',
    env: {},
    fileExists: () => true,
    platform: 'linux',
  });

  assert.deepEqual(resolved.env, {});
  assert.equal(resolved.libraryDirectory, undefined);
});

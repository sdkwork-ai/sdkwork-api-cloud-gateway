import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import {
  assertRemoteCloudDevelopmentProfile,
  resolveGatewaySourceProfile,
} from './sdkwork-api-cloud-gateway-source-config.mjs';

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

test('standalone development resolves the tracked PostgreSQL gateway profile', () => {
  const profile = resolveGatewaySourceProfile({
    appRoot,
    deploymentProfile: 'standalone',
    environment: 'development',
  });

  assert.equal(profile.profileId, 'standalone.development');
  assert.equal(profile.env.SDKWORK_DATABASE_ENGINE, 'postgresql');
  assert.equal(profile.runtimeConfig, 'etc/sdkwork-api-cloud-gateway.development.toml');
  assert.ok(profile.runtimeConfigPath.endsWith('sdkwork-api-cloud-gateway.development.toml'));
  assert.ok(!profile.runtimeConfig.includes('.example'));
});

test('cloud development selects only an explicit remote gateway', () => {
  const profile = resolveGatewaySourceProfile({
    appRoot,
    deploymentProfile: 'cloud',
    environment: 'development',
  });

  assert.doesNotThrow(() => assertRemoteCloudDevelopmentProfile(profile));
  assert.equal(profile.runtimeConfig, undefined);
  assert.equal(
    profile.env.SDKWORK_API_CLOUD_GATEWAY_PUBLIC_HTTP_URL,
    'https://api-dev.sdkwork.com',
  );
});

test('source profile selection fails closed for undeclared combinations', () => {
  assert.throws(
    () => resolveGatewaySourceProfile({
      appRoot,
      deploymentProfile: 'cloud',
      environment: 'test',
    }),
    /does not declare profile cloud\.test/u,
  );
});

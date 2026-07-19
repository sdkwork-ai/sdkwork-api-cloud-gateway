import { readFileSync } from 'node:fs';
import path from 'node:path';

import { parseDotEnv } from '../../../sdkwork-specs/tools/postgres/postgres-config.mjs';

const deploymentProfiles = new Set(['standalone', 'cloud']);
const lifecycleEnvironments = new Set(['development', 'test', 'staging', 'production']);

export function resolveGatewaySourceProfile({ appRoot, deploymentProfile, environment }) {
  if (!deploymentProfiles.has(deploymentProfile)) {
    throw new Error(`Unsupported deployment profile: ${deploymentProfile}`);
  }
  if (!lifecycleEnvironments.has(environment)) {
    throw new Error(`Unsupported lifecycle environment: ${environment}`);
  }

  const etcRoot = path.resolve(appRoot, 'etc');
  const indexPath = path.join(etcRoot, 'sdkwork.deployment.config.json');
  const index = JSON.parse(readFileSync(indexPath, 'utf8'));
  const profileId = `${deploymentProfile}.${environment}`;
  const profile = index.profiles?.[profileId];
  if (!profile?.config) {
    throw new Error(`Source configuration does not declare profile ${profileId}`);
  }

  const profilePath = resolveInsideEtc(etcRoot, profile.config, `${profileId} config`);
  const env = parseDotEnv(readFileSync(profilePath, 'utf8'));
  assertProfileValue(env, 'SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE', deploymentProfile);
  assertProfileValue(env, 'SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT', environment);
  assertProfileValue(env, 'SDKWORK_ENVIRONMENT', environment);
  assertProfileValue(env, 'SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID', profileId);

  const runtimeConfig = env.SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE;
  const runtimeConfigPath = runtimeConfig
    ? resolveInsideAppRoot(appRoot, runtimeConfig, 'gateway runtime config')
    : undefined;

  return {
    deploymentProfile,
    env,
    environment,
    indexPath,
    profileId,
    profilePath,
    runtimeConfig,
    runtimeConfigPath,
  };
}

export function assertRemoteCloudDevelopmentProfile(profile) {
  if (profile.profileId !== 'cloud.development') {
    return;
  }
  const value = profile.env.SDKWORK_API_CLOUD_GATEWAY_PUBLIC_HTTP_URL;
  const url = new URL(value ?? '');
  if (url.protocol !== 'https:' || isLoopbackHost(url.hostname)) {
    throw new Error('cloud.development requires an explicit remote HTTPS gateway URL');
  }
  if (profile.runtimeConfig) {
    throw new Error('cloud.development must not select a local gateway runtime config');
  }
}

function assertProfileValue(env, key, expected) {
  if (env[key] !== expected) {
    throw new Error(`${key} must be ${expected} in the selected source profile`);
  }
}

function resolveInsideEtc(etcRoot, relativePath, label) {
  const resolved = path.resolve(etcRoot, relativePath);
  const relative = path.relative(etcRoot, resolved);
  if (relative.startsWith('..') || path.isAbsolute(relative)) {
    throw new Error(`${label} must stay inside etc/`);
  }
  return resolved;
}

function resolveInsideAppRoot(appRoot, relativePath, label) {
  const resolvedRoot = path.resolve(appRoot);
  const resolved = path.resolve(resolvedRoot, relativePath);
  const relative = path.relative(resolvedRoot, resolved);
  if (relative.startsWith('..') || path.isAbsolute(relative)) {
    throw new Error(`${label} must stay inside the application root`);
  }
  return resolved;
}

function isLoopbackHost(hostname) {
  return ['localhost', '127.0.0.1', '0.0.0.0', '::1', '[::1]'].includes(hostname);
}

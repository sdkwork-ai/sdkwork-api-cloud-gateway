import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';

import { parseDotEnv } from '../../sdkwork-specs/tools/postgres/postgres-config.mjs';
import { validateDeploymentEnvironment } from './validate-docker-deployment.mjs';

const appRoot = path.resolve('.');
const environments = ['development', 'test', 'staging', 'production'];
const profiles = ['standalone', 'cloud'];
const baseDomains = ['sdkwork.com', 'birdcoder.com', 'dtupay.com'];
const suffixes = { development: 'dev', test: 'test', staging: 'staging', production: '' };

function expectedHosts(environment) {
  const role = suffixes[environment] ? `api-${suffixes[environment]}` : 'api';
  return baseDomains.map((domain) => `${role}.${domain}`);
}

test('deployment index and topology expose the complete canonical profile matrix', () => {
  const index = JSON.parse(readFileSync(path.join(appRoot, 'etc', 'sdkwork.deployment.config.json')));
  const topology = JSON.parse(readFileSync(path.join(appRoot, 'specs', 'topology.spec.json')));
  for (const environment of environments) {
    assert.deepEqual(index.environments[environment].publicHosts, expectedHosts(environment));
    for (const deploymentProfile of profiles) {
      const profileId = `${deploymentProfile}.${environment}`;
      assert.equal(index.profiles[profileId].config, `topology/${profileId}.env`);
      assert.equal(topology.profileFiles[profileId], `etc/topology/${profileId}.env`);
      assert.ok(topology.orchestration.profiles[profileId]);
    }
  }
  assert.deepEqual(
    topology.cloudPublicHosts['platform.api-gateway'].httpHosts,
    expectedHosts('production'),
  );
});

test('docker examples bind exactly the registered host family', () => {
  for (const environment of environments) {
    const file = path.join(appRoot, 'docker', 'env', `${environment}.env.example`);
    const env = parseDotEnv(readFileSync(file, 'utf8'));
    const resolved = { ...env };
    resolved.GATEWAY_IMAGE = 'sdkwork-api-cloud-gateway:test';
    resolved.GATEWAY_POSTGRES_PASSWORD = 'test-only-password';
    for (const key of Object.keys(resolved)) {
      if (resolved[key] === '<CHANGE_ME>') {
        resolved[key] = `test-only-${key.toLowerCase()}`;
      }
    }
    const summary = validateDeploymentEnvironment(resolved, 'embedded');
    assert.deepEqual(summary.hosts, expectedHosts(environment));
  }
});

test('external compose mode disables embedded dependencies and requires explicit hosts', () => {
  const override = readFileSync(path.join(appRoot, 'docker-compose.external.yml'), 'utf8');
  assert.match(override, /profiles: \["external-disabled"\]/u);
  assert.match(override, /GATEWAY_POSTGRES_HOST:\?/u);
  assert.match(override, /GATEWAY_CLOUDROUTER_REDIS_HOST:\?/u);
  assert.match(override, /depends_on: !reset \[\]/u);
});

test('nginx assets bind each primary domain and its two aliases', () => {
  const ports = { development: 3910, test: 3911, staging: 3912, production: 3913 };
  for (const environment of environments) {
    const hosts = expectedHosts(environment);
    const nginx = readFileSync(path.join(appRoot, 'docker', 'nginx', `${hosts[0]}.conf`), 'utf8');
    assert.match(nginx, new RegExp(`server_name ${hosts.join(' ')};`, 'u'));
    assert.match(nginx, new RegExp(`127\\.0\\.0\\.1:${ports[environment]}`, 'u'));
  }
});

test('application manifest declares explicit package build targets', () => {
  const manifest = JSON.parse(readFileSync(path.join(appRoot, 'sdkwork.app.config.json')));
  const targets = manifest.devApp.build.targets;
  assert.deepEqual(targets.map((target) => target.packageId), [
    'linux-x64-standalone-server-tar-gz',
    'linux-x64-cloud-server-tar-gz',
  ]);
  assert.ok(targets.every((target) => target.outputs.length === 1));
  assert.equal(manifest.security.checksumRequired, false);
});

test('Ubuntu installer uses canonical runtime paths and preserves operator config', () => {
  const installer = readFileSync(path.join(appRoot, 'scripts', 'install-ubuntu.sh'), 'utf8');
  assert.match(installer, /\/opt\/sdkwork\/api-gateway/u);
  assert.match(installer, /\/etc\/sdkwork\/api-gateway/u);
  assert.match(installer, /\/var\/lib\/sdkwork\/api-gateway/u);
  assert.match(installer, /if \[ ! -e "\$config_target" \]/u);
  assert.match(installer, /--destdir/u);
});

test('database lifecycle is explicit and includes Models before CloudRouter', () => {
  const compose = readFileSync(path.join(appRoot, 'docker-compose.yml'), 'utf8');
  const entrypoint = readFileSync(path.join(appRoot, 'container', 'entrypoint'), 'utf8');
  const systemd = readFileSync(
    path.join(appRoot, 'deployments', 'systemd', 'sdkwork-api-gateway.service'),
    'utf8',
  );
  const gatewayCargo = readFileSync(
    path.join(appRoot, 'crates', 'sdkwork-api-cloud-gateway', 'Cargo.toml'),
    'utf8',
  );
  const bootstrap = readFileSync(
    path.join(
      appRoot,
      'crates',
      'sdkwork-api-cloud-gateway',
      'src',
      'embedded_database_bootstrap.rs',
    ),
    'utf8',
  );

  assert.match(compose, /SDKWORK_API_CLOUD_GATEWAY_MIGRATE_ON_START:/u);
  assert.match(compose, /GATEWAY_DATABASE_AUTO_MIGRATE:-false/u);
  assert.match(entrypoint, /sdkwork-api-cloud-gateway --migrate-databases/u);
  assert.match(systemd, /ExecStartPre=.* --migrate-databases/u);
  assert.match(gatewayCargo, /"dep:sdkwork-models-database-host"/u);
  assert.ok(
    bootstrap.indexOf('migrate_models_database') <
      bootstrap.indexOf('migrate_cloud_router_database'),
    'Models must migrate before CloudRouter validates its catalog schema',
  );
});

test('server install manifest declares its self-referential checksum exclusions', () => {
  const packager = readFileSync(path.join(appRoot, 'scripts', 'package-server.mjs'), 'utf8');
  assert.match(
    packager,
    /contentManifestExclusions:\s*\[\s*'install-manifest\.json',\s*'checksums\.sha256'/u,
  );
});

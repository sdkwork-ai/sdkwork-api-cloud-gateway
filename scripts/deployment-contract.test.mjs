import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { parse } from 'yaml';

import { parseDotEnv } from '../../sdkwork-specs/tools/postgres/postgres-config.mjs';
import {
  ENVIRONMENTS,
  ENVIRONMENT_HOST_PORTS,
} from './deployment-environments.mjs';
import {
  resolveExampleEnvironment,
  validateDeploymentEnvironment,
  validateDeploymentMatrix,
} from './validate-docker-deployment.mjs';

const appRoot = path.resolve('.');
const environments = ENVIRONMENTS;
const profiles = ['standalone', 'cloud'];
const baseDomains = ['sdkwork.com', 'birdcoder.com', 'dtupay.com'];
const suffixes = {
  development: 'dev',
  test: 'test',
  staging: 'staging',
  demo: 'demo',
  production: '',
};

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
  const topologyHosts = topology.cloudPublicHosts['platform.api-gateway'].httpHosts;
  for (const host of expectedHosts('production')) {
    assert.ok(topologyHosts.includes(host), `topology missing ${host}`);
  }
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
    assert.equal(resolved.GATEWAY_IM_PRINCIPAL_DIRECTORY, 'postgres');
    assert.equal(resolved.GATEWAY_IM_ALLOW_ALL_PRINCIPALS, 'false');
    assert.match(resolved.GATEWAY_IM_ID_NODE_ID, /^(?:[0-9]|[1-9][0-9]{1,2}|10[01][0-9]|102[0-3])$/u);
    assert.ok(Number.parseInt(resolved.GATEWAY_KNOWLEDGEBASE_TENANT_ID, 10) > 0);
    assert.ok(
      resolved.GATEWAY_KNOWLEDGEBASE_ORGANIZATION_ID !== undefined
        && /^(?:0|[1-9][0-9]*)$/u.test(resolved.GATEWAY_KNOWLEDGEBASE_ORGANIZATION_ID),
      'GATEWAY_KNOWLEDGEBASE_ORGANIZATION_ID must be a nonnegative integer',
    );
  }
});

test('container deployment matrix covers both dependency modes in every environment', () => {
  const summaries = validateDeploymentMatrix(appRoot);
  assert.equal(summaries.length, environments.length * 2);
  assert.deepEqual(new Set(summaries.map(({ mode }) => mode)), new Set(['embedded', 'external']));
  assert.deepEqual(
    new Set(summaries.map(({ environment }) => environment)),
    new Set(environments),
  );

  const production = parseDotEnv(readFileSync(
    path.join(appRoot, 'docker', 'env', 'production.env.example'),
    'utf8',
  ));
  const external = resolveExampleEnvironment(production, 'external');
  assert.doesNotMatch(JSON.stringify(external), /<[^>]+>/u);
  assert.equal(external.GATEWAY_POSTGRES_HOST, 'postgres.validation.internal');
  assert.equal(external.GATEWAY_CLOUDROUTER_REDIS_HOST, 'redis.validation.internal');
});

test('derived PostgreSQL URLs reject reserved credentials unless an encoded URL is explicit', () => {
  const source = parseDotEnv(readFileSync(
    path.join(appRoot, 'docker', 'env', 'development.env.example'),
    'utf8',
  ));
  const env = resolveExampleEnvironment(source, 'embedded');
  env.GATEWAY_POSTGRES_PASSWORD = 'unsafe/password';
  assert.throws(
    () => validateDeploymentEnvironment(env, 'embedded'),
    /set an encoded SDKWORK_DATABASE_URL/u,
  );

  env.SDKWORK_DATABASE_URL =
    'postgresql://sdkwork_ai_dev:unsafe%2Fpassword@postgres:5432/sdkwork_ai_dev?sslmode=disable&options=-c%20search_path%3Dsdkwork_ai_dev';
  assert.doesNotThrow(() => validateDeploymentEnvironment(env, 'embedded'));

  delete env.SDKWORK_DATABASE_URL;
  env.GATEWAY_POSTGRES_PASSWORD = '';
  env.GATEWAY_POSTGRES_PASSWORD_FILE = '/run/secrets/sdkwork/database.password';
  assert.throws(
    () => validateDeploymentEnvironment(env, 'embedded'),
    /SDKWORK_DATABASE_URL is required/u,
  );
});

test('database, schema, and role identities stay canonical for every environment', () => {
  const source = parseDotEnv(readFileSync(
    path.join(appRoot, 'docker', 'env', 'development.env.example'),
    'utf8',
  ));
  const env = resolveExampleEnvironment(source, 'external');
  env.GATEWAY_POSTGRES_USER = 'managed_gateway_user';
  assert.throws(
    () => validateDeploymentEnvironment(env, 'external'),
    /GATEWAY_POSTGRES_USER must be sdkwork_ai_dev/u,
  );
});

test('all source topology profiles select the durable IM principal directory', () => {
  for (const environment of environments) {
    for (const deploymentProfile of profiles) {
      const profileId = `${deploymentProfile}.${environment}`;
      const env = parseDotEnv(
        readFileSync(path.join(appRoot, 'etc', 'topology', `${profileId}.env`), 'utf8'),
      );
      assert.equal(env.SDKWORK_IM_PRINCIPAL_DIRECTORY, 'postgres', profileId);
    }
  }
});

test('default compose uses external PostgreSQL/Redis; embedded is opt-in', () => {
  const compose = readFileSync(path.join(appRoot, 'docker-compose.yml'), 'utf8');
  const embedded = readFileSync(path.join(appRoot, 'docker-compose.embedded.yml'), 'utf8');
  const externalShim = readFileSync(path.join(appRoot, 'docker-compose.external.yml'), 'utf8');
  assert.match(compose, /profiles: \["embedded"\]/u);
  assert.match(compose, /GATEWAY_POSTGRES_HOST:-host\.docker\.internal/u);
  assert.match(compose, /GATEWAY_CLOUDROUTER_REDIS_HOST:-host\.docker\.internal/u);
  assert.match(compose, /GATEWAY_CLOUDROUTER_REDIS_PORT:-6380/u);
  assert.match(compose, /SDKWORK_CLOUDROUTER_REDIS_PASSWORD: \$\{GATEWAY_CLOUDROUTER_REDIS_PASSWORD:-\}/u);
  assert.match(compose, /\/etc\/sdkwork\/api-gateway\/config\.toml/u);
  assert.match(compose, /host\.docker\.internal:host-gateway/u);
  assert.match(compose, /sdkwork-webserver is an independent Docker deployment/u);
  assert.doesNotMatch(compose, /SDKWORK_WEBSERVER_/u);
  assert.match(embedded, /SDKWORK_CLOUDROUTER_REDIS_HOST: redis/u);
  assert.match(embedded, /SDKWORK_CLOUDROUTER_REDIS_PASSWORD: ""/u);
  assert.match(externalShim, /external PostgreSQL\/Redis is now the default/u);
});

test('container image packages the official Knowledgebase RPC sidecar', () => {
  const buildScript = readFileSync(
    path.join(appRoot, 'scripts', 'build-api-cloud-gateway-container.mjs'),
    'utf8',
  );
  const dockerfile = readFileSync(path.join(appRoot, 'Dockerfile'), 'utf8');

  assert.match(buildScript, /sdkwork-intelligence-knowledgebase-rpc-bin/u);
  assert.match(buildScript, /sdkwork-knowledgebase/u);
  assert.match(buildScript, /knowledgebaseRpcBinaryPath/u);
  assert.match(buildScript, /container\/knowledgebase-rpc-entrypoint/u);
  assert.match(dockerfile, /KNOWLEDGEBASE_RPC_BINARY/u);
  assert.match(dockerfile, /container\/knowledgebase-rpc-entrypoint/u);
});

test('container composition wires the IM Knowledgebase client through strict RPC security', () => {
  const compose = readFileSync(path.join(appRoot, 'docker-compose.yml'), 'utf8');
  const entrypoint = readFileSync(
    path.join(appRoot, 'container', 'knowledgebase-rpc-entrypoint'),
    'utf8',
  );

  assert.match(compose, /knowledgebase-rpc:/u);
  assert.match(compose, /SDKWORK_KNOWLEDGEBASE_RPC_MTLS_ENABLED: "true"/u);
  assert.match(compose, /SDKWORK_KNOWLEDGEBASE_RPC_SPIFFE_TRUST_DOMAIN: sdkwork\.internal/u);
  assert.match(compose, /SDKWORK_KNOWLEDGEBASE_TENANT_ID:/u);
  assert.match(compose, /SDKWORK_KNOWLEDGEBASE_DEPLOYMENT_PROFILE: standalone/u);
  assert.match(compose, /SDKWORK_KNOWLEDGEBASE_SECRETS_ENCRYPTION_KEY_FILE:/u);
  assert.doesNotMatch(compose, /SDKWORK_WEBSERVER_/u);
  assert.match(compose, /sdkwork-webserver is an independent Docker deployment/u);
  assert.match(compose, /SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_ENDPOINT:/u);
  assert.match(compose, /SDKWORK_IM_ID_NODE_ID:/u);
  assert.match(compose, /SDKWORK_IM_GROUP_KNOWLEDGEBASE_LAUNCH_TICKET_SECRET_FILE:/u);
  assert.match(compose, /SDKWORK_DATABASE_URL: \$\{SDKWORK_DATABASE_URL:-postgresql:/u);
  assert.match(compose, /SDKWORK_RTC_STATE_REDIS_URL:/u);
  assert.match(compose, /SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CA_CERT_PATH:/u);
  assert.match(compose, /SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CERT_PATH:/u);
  assert.match(compose, /SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_KEY_PATH:/u);
  assert.match(compose, /SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_TLS_DOMAIN:/u);
  assert.match(
    compose,
    /SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CALLER_CONTEXT_SIGNING_KEY_FILE:/u,
  );
  assert.match(compose, /gateway:\s+[\s\S]*knowledgebase-rpc:\s+condition: service_healthy/u);
  assert.match(entrypoint, /URI:spiffe:\/\/sdkwork\.internal\/sdkwork\/service\/sdkwork-im/u);
  assert.match(entrypoint, /subjectAltName=DNS:knowledgebase\.local,DNS:knowledgebase-rpc/u);
  assert.match(entrypoint, /openssl rand -base64 32/u);
  assert.match(entrypoint, /openssl rand -base64 48/u);
  assert.match(entrypoint, /knowledgebase-secrets\.key/u);
  assert.match(entrypoint, /webserver-secrets\.key/u);
  assert.match(entrypoint, /chmod 0600 "\$secrets_tmp"\/\*\.key/u);
  assert.match(entrypoint, /mv "\$secrets_tmp" "\$secrets_root"/u);
});

test('container entrypoint does not bootstrap sdkwork-webserver secrets', () => {
  const gatewayEntrypoint = readFileSync(
    path.join(appRoot, 'container', 'entrypoint'),
    'utf8',
  );
  const dockerfile = readFileSync(path.join(appRoot, 'Dockerfile'), 'utf8');
  const envExample = readFileSync(path.join(appRoot, 'docker', '.env.example'), 'utf8');
  assert.doesNotMatch(gatewayEntrypoint, /load_webserver_secret/u);
  assert.doesNotMatch(gatewayEntrypoint, /SDKWORK_WEBSERVER_/u);
  assert.doesNotMatch(dockerfile, /SDKWORK_WEBSERVER_/u);
  assert.doesNotMatch(envExample, /^GATEWAY_WEBSERVER_/mu);
});

test('ubuntu and container share api-gateway config layout; gateway does not own nginx', () => {
  const installer = readFileSync(path.join(appRoot, 'scripts', 'install-ubuntu.sh'), 'utf8');
  const compose = readFileSync(path.join(appRoot, 'docker-compose.yml'), 'utf8');
  const apply = readFileSync(path.join(appRoot, 'scripts', 'deploy', 'apply.sh'), 'utf8');
  const ports = ENVIRONMENT_HOST_PORTS;

  assert.match(installer, /config_root=\$destdir\/etc\/sdkwork\/api-gateway/u);
  assert.match(installer, /config_target=\$config_root\/config\.toml/u);
  assert.match(installer, /env_target=\$config_root\/api-gateway\.env/u);
  assert.match(compose, /\/etc\/sdkwork\/api-gateway\/config\.toml/u);
  assert.match(compose, /SDKWORK_CLOUDROUTER_REDIS_PASSWORD: \$\{GATEWAY_CLOUDROUTER_REDIS_PASSWORD:-\}/u);
  assert.match(apply, /does NOT generate or install nginx/u);

  for (const environment of environments) {
    const example = parseDotEnv(
      readFileSync(path.join(appRoot, 'docker', 'env', `${environment}.env.example`), 'utf8'),
    );
    assert.equal(example.GATEWAY_HOST_PORT, String(ports[environment]));
    assert.equal(example.GATEWAY_CLOUDROUTER_REDIS_PASSWORD, '');
    assert.equal(example.GATEWAY_CLOUDROUTER_REDIS_HOST, 'host.docker.internal');
    assert.equal(example.GATEWAY_POSTGRES_HOST, 'host.docker.internal');
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
  assert.deepEqual(manifest.security, {
    checksumRequired: true,
    signatureRequired: true,
    sbomRequired: true,
  });

  const workflow = JSON.parse(readFileSync(path.join(appRoot, 'sdkwork.workflow.json')));
  assert.deepEqual(workflow.security, {
    artifactAttestations: true,
    sbomRequired: true,
    signingRequired: true,
  });
  assert.ok(workflow.lifecycle.sign.some((step) => /release-evidence\.mjs sign/u.test(step.run)));
  assert.ok(workflow.lifecycle.sbom.some((step) => /release-evidence\.mjs sbom/u.test(step.run)));
  assert.ok(workflow.targets.every((target) => typeof target.artifactPath === 'string'));
});

test('packaged binary help and version bypass runtime initialization', () => {
  const listenerMain = readFileSync(
    path.join(
      appRoot,
      'crates',
      'sdkwork-api-cloud-gateway',
      'src',
      'listener_main.rs',
    ),
    'utf8',
  );
  assert.match(listenerMain, /ProcessMode::Help\s*=>\s*\{[\s\S]*return Ok\(\(\)\)/u);
  assert.match(listenerMain, /ProcessMode::Version\s*=>\s*\{[\s\S]*return Ok\(\(\)\)/u);
  assert.ok(
    listenerMain.indexOf('match process_mode')
      < listenerMain.indexOf('install_process_crypto_provider()?'),
    'help/version dispatch must happen before process runtime initialization',
  );
});

test('Ubuntu installer uses canonical runtime paths and preserves operator config', () => {
  const installer = readFileSync(path.join(appRoot, 'scripts', 'install-ubuntu.sh'), 'utf8');
  const gatewayUnit = readFileSync(
    path.join(appRoot, 'deployments', 'systemd', 'sdkwork-api-gateway.service'),
    'utf8',
  );
  const knowledgebaseUnit = readFileSync(
    path.join(appRoot, 'deployments', 'systemd', 'sdkwork-knowledgebase-rpc.service'),
    'utf8',
  );
  assert.match(installer, /\/opt\/sdkwork\/api-gateway/u);
  assert.match(installer, /\/etc\/sdkwork\/api-gateway/u);
  assert.match(installer, /\/var\/lib\/sdkwork\/api-gateway/u);
  assert.match(installer, /config\.toml/u);
  assert.match(installer, /api-gateway\.env/u);
  assert.match(installer, /if \[ ! -e "\$config_target" \]/u);
  assert.match(installer, /--destdir/u);
  assert.match(installer, /SDKWORK_IM_PRINCIPAL_DIRECTORY=postgres/u);
  assert.match(installer, /sdkwork-intelligence-knowledgebase-rpc-bin/u);
  assert.match(installer, /SDKWORK_DATABASE_URL_FILE=\/etc\/sdkwork\/database\/database\.url\.secret/u);
  assert.match(installer, /knowledgebase-rpc-entrypoint.*--provision-only/u);
  assert.match(installer, /"\$binary_source" --version >\/dev\/null/u);
  assert.match(gatewayUnit, /EnvironmentFile=\/etc\/sdkwork\/api-gateway\/api-gateway\.env/u);
  assert.match(gatewayUnit, /Requires=sdkwork-knowledgebase-rpc\.service/u);
  assert.match(knowledgebaseUnit, /ExecStart=.*knowledgebase-rpc-entrypoint/u);
});

test('database lifecycle is explicit and delegated to owner assemblies', () => {
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
  assert.match(compose, /SDKWORK_DATABASE_AUTO_MIGRATE:-false/u);
  assert.match(entrypoint, /sdkwork-api-cloud-gateway --migrate-databases/u);
  assert.match(systemd, /ExecStartPre=.* --migrate-databases/u);
  assert.doesNotMatch(gatewayCargo, /database-host/u);
  assert.doesNotMatch(bootstrap, /_database_host::/u);
  assert.match(
    bootstrap,
    /bootstrap_embedded_dependency_contributions\(/u,
    'installer lifecycle must use the same owner assembly integration point as runtime',
  );
});

test('single-node containers provision the Payment key in the canonical secret mount', () => {
  const compose = readFileSync(path.join(appRoot, 'docker-compose.yml'), 'utf8');
  const entrypoint = readFileSync(path.join(appRoot, 'container', 'entrypoint'), 'utf8');

  assert.match(
    compose,
    /SDKWORK_PAYMENT_CREDENTIAL_MASTER_KEY_FILE: \/run\/secrets\/sdkwork\/api-gateway\//u,
  );
  assert.match(compose, /gateway-secrets:\/run\/secrets\/sdkwork\/api-gateway/u);
  assert.match(compose, /GATEWAY_PROVISION_PAYMENT_CREDENTIAL_KEY:-true/u);
  assert.match(
    compose,
    /SDKWORK_IM_PRINCIPAL_DIRECTORY: \$\{GATEWAY_IM_PRINCIPAL_DIRECTORY:-postgres\}/u,
  );
  assert.match(entrypoint, /SDKWORK_API_CLOUD_GATEWAY_PROVISION_PAYMENT_CREDENTIAL_KEY/u);
  assert.match(entrypoint, /\/run\/secrets\/sdkwork\/api-gateway\/\*/u);
  assert.match(entrypoint, /umask 077/u);
  assert.match(entrypoint, /dd if=\/dev\/urandom bs=32 count=1/u);
  assert.match(entrypoint, /set -C/u);
});

test('production-like embedded PostgreSQL enables transport encryption', () => {
  const compose = readFileSync(path.join(appRoot, 'docker-compose.yml'), 'utf8');
  const embedded = readFileSync(path.join(appRoot, 'docker-compose.embedded.yml'), 'utf8');
  const postgresEntrypoint = readFileSync(
    path.join(appRoot, 'docker', 'postgres', 'entrypoint.sh'),
    'utf8',
  );

  assert.match(compose, /ssl=on/u);
  assert.match(compose, /ssl_cert_file=\/var\/lib\/postgresql\/server\.crt/u);
  assert.match(compose, /docker\/postgres\/entrypoint\.sh:/u);
  assert.match(compose, /profiles: \["embedded"\]/u);
  assert.match(
    embedded,
    /POSTGRES_CA_FILE: \/run\/sdkwork\/postgres-tls\/server\.crt/u,
  );
  // The merged trust bundle must live OFF the ro postgres-ca-bundle.crt bind
  // mount target: the entrypoint writes <trust>.tmp.$$ beside it and mv would
  // fail on a read-only mount (startup crash loop).
  assert.match(
    embedded,
    /SSL_CERT_FILE: \/run\/sdkwork\/api-gateway\/postgres-ca-bundle\.merged\.crt/u,
  );
  assert.match(embedded, /postgres-tls:\/run\/sdkwork\/postgres-tls:ro/u);
  assert.match(postgresEntrypoint, /openssl req -new -x509 -nodes/u);
  assert.match(postgresEntrypoint, /chmod 0600 "\$private_key_tmp"/u);
  assert.match(postgresEntrypoint, /umask "\$original_umask"/u);
  assert.match(postgresEntrypoint, /cp "\$certificate" "\$shared_certificate_tmp"/u);
  const gatewayEntrypoint = readFileSync(path.join(appRoot, 'container', 'entrypoint'), 'utf8');
  assert.match(
    gatewayEntrypoint,
    /cat "\$system_trust_bundle" "\$ca_file" > "\$trust_bundle_tmp"/u,
  );
  for (const environment of ['staging', 'production']) {
    const env = parseDotEnv(
      readFileSync(path.join(appRoot, 'docker', 'env', `${environment}.env.example`), 'utf8'),
    );
    assert.equal(env.GATEWAY_POSTGRES_SSL_MODE, 'require');
  }
});

test('server install manifest declares its self-referential checksum exclusions', () => {
  const packager = readFileSync(path.join(appRoot, 'scripts', 'package-server.mjs'), 'utf8');
  assert.match(
    packager,
    /contentManifestExclusions:\s*\[\s*'install-manifest\.json',\s*'checksums\.sha256'/u,
  );
});

test('bundle compose mounts the PostgreSQL CA trust bundle like the repo layout', () => {
  const bundle = readFileSync(path.join(appRoot, 'docker-compose.bundle.yml'), 'utf8');
  const repo = readFileSync(path.join(appRoot, 'docker-compose.yml'), 'utf8');
  // The bundle distribution must provide the same external-TLS trust anchor as
  // docker-compose.yml: GATEWAY_POSTGRES_SSL_CERT_FILE points at the mounted
  // bundle path, so dropping the mount silently breaks sslmode=require pools.
  // The default mount source is /dev/null in BOTH layouts: an absent host
  // source would make docker create a stray directory, and a real default file
  // mount collides with the entrypoint's merged trust-bundle write target.
  for (const compose of [bundle, repo]) {
    assert.match(
      compose,
      /SSL_CERT_FILE: \$\{GATEWAY_POSTGRES_SSL_CERT_FILE:-\/etc\/ssl\/certs\/ca-certificates\.crt\}/u,
    );
    assert.match(
      compose,
      /\$\{GATEWAY_POSTGRES_CA_FILE_MOUNT:-\/dev\/null\}:\/run\/sdkwork\/api-gateway\/postgres-ca-bundle\.crt:ro/u,
    );
  }
});

test('install bundle supports external (default) and embedded dependency modes', () => {
  // The environment-neutral bundle template ships no database services:
  // external PostgreSQL + Redis is the only posture of the base file.
  const bundle = readFileSync(path.join(appRoot, 'docker-compose.bundle.yml'), 'utf8');
  assert.doesNotMatch(bundle, /pgvector/u);
  assert.doesNotMatch(bundle, /redis:8-alpine/u);
  assert.match(bundle, /GATEWAY_POSTGRES_HOST:-host\.docker\.internal/u);

  // The embedded override adds the built-in PostgreSQL (pgvector, TLS) and
  // Redis pair to the per-environment deps project and repoints the gateway
  // and knowledgebase-rpc at them; instance projects never start their own
  // database copies (profiles: ["deps"]).
  const embedded = readFileSync(
    path.join(appRoot, 'docker-compose.bundle.embedded.yml'),
    'utf8',
  );
  assert.match(embedded, /pgvector\/pgvector:pg18/u);
  assert.match(embedded, /redis:8-alpine/u);
  assert.match(embedded, /ssl=on/u);
  // The embedded default connection string must honor GATEWAY_POSTGRES_SSL_MODE
  // (P0-12 production fail-closed requires sslmode=require/verify-full pools);
  // the literal "disable" default keeps dev/test/demo postures unchanged.
  assert.match(embedded, /sslmode=\$\{GATEWAY_POSTGRES_SSL_MODE:-disable\}/u);
  assert.match(embedded, /profiles: \["deps"\]/u);
  assert.match(embedded, /SDKWORK_DATABASE_HOST: postgres/u);
  assert.match(embedded, /SDKWORK_CLOUDROUTER_REDIS_HOST: redis/u);
  assert.match(embedded, /SDKWORK_RTC_STATE_REDIS_URL: redis:\/\/redis:6379\/1/u);
  assert.match(embedded, /postgres-tls:\/run\/sdkwork\/postgres-tls:ro/u);
  assert.match(embedded, /POSTGRES_ASSETS_DIR:\?required/u);
  assert.match(
    embedded,
    /name: sdkwork-api-cloud-gateway-\$\{GATEWAY_ENVIRONMENT:\?required\}-postgres-data/u,
  );

  // deploy.sh: external is the default; --embedded is the explicit opt-in and
  // GATEWAY_DEPS_MODE=embedded in the env file is equivalent.
  const deploy = readFileSync(path.join(appRoot, 'scripts', 'docker', 'bundle', 'deploy.sh'), 'utf8');
  assert.match(deploy, /DEPS_MODE="\$\{DEPS_MODE_FLAG:-\$\(env_key GATEWAY_DEPS_MODE\)\}"/u);
  assert.match(deploy, /DEPS_MODE="\$\{DEPS_MODE:-external\}"/u);
  assert.match(deploy, /--embedded\)\s+DEPS_MODE_FLAG="embedded"/u);
  assert.match(deploy, /docker-compose\.bundle\.embedded\.yml/u);
  assert.match(deploy, /POSTGRES_ASSETS_DIR/u);
  assert.match(deploy, /postgres-data redis-data postgres-tls/u);

  // The packager ships the override plus the embedded postgres assets.
  const packager = readFileSync(path.join(appRoot, 'scripts', 'docker', 'package-install-bundle.mjs'), 'utf8');
  assert.match(packager, /docker-compose\.bundle\.embedded\.yml/u);
  assert.match(packager, /postgres/, 'packager must ship the embedded postgres assets');
  assert.match(packager, /dependencyModes: \['external', 'embedded'\]/u);
});

test('compose passes the cloudrouter web environment through to the container', () => {
  const repo = readFileSync(path.join(appRoot, 'docker-compose.yml'), 'utf8');
  const bundle = readFileSync(path.join(appRoot, 'docker-compose.bundle.yml'), 'utf8');
  const expected = /SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT: \$\{GATEWAY_CLOUDROUTER_ROUTER_ENVIRONMENT:-\}/u;
  assert.match(repo, expected);
  assert.match(bundle, expected);
  // Every env file (and example) declares its router environment explicitly:
  // unset or unknown values fail closed to the production posture inside
  // cloudrouter's web_security parse_environment.
  const envDir = path.join(appRoot, 'docker', 'env');
  for (const name of ['development', 'test', 'staging', 'demo', 'production']) {
    for (const suffix of ['.env', '.env.example']) {
      const file = readFileSync(path.join(envDir, `${name}${suffix}`), 'utf8');
      assert.match(
        file,
        new RegExp(`^GATEWAY_CLOUDROUTER_ROUTER_ENVIRONMENT=${name}$`, 'mu'),
        `${name}${suffix} must declare GATEWAY_CLOUDROUTER_ROUTER_ENVIRONMENT=${name}`,
      );
    }
  }
});

test('shipped compose files bound container logs (DEPLOYMENT_SPEC §6 runtime hardening)', () => {
  // Normative: every service in every compose file shipped for deployment MUST
  // declare bounded json-file log rotation (reference 50m/3) plus a restart
  // policy and a healthcheck. Overlays (docker-compose.embedded.yml,
  // docker-compose.external.yml) merge into the base file and are covered by
  // it. Override files only need explicit logging for services they ADD;
  // services shared with the base file inherit it through compose merge.
  const shipped = [
    { file: 'docker-compose.yml' },
    { file: 'docker-compose.bundle.yml' },
    {
      file: 'docker-compose.bundle.embedded.yml',
      baseFile: 'docker-compose.bundle.yml',
    },
  ];
  for (const { file, baseFile } of shipped) {
    // merge: true expands YAML `<<` anchors (the bundle composition shares
    // logging through the x-gateway-common anchor).
    const compose = parse(readFileSync(path.join(appRoot, file), 'utf8'), { merge: true });
    const baseServices = baseFile
      ? Object.keys(parse(readFileSync(path.join(appRoot, baseFile), 'utf8'), { merge: true }).services ?? {})
      : [];
    const services = Object.keys(compose.services ?? {});
    assert.ok(services.length > 0, `${file} must declare services`);
    for (const [service, definition] of Object.entries(compose.services)) {
      if (baseServices.includes(service)) {
        continue;
      }
      assert.equal(
        definition.logging?.driver,
        'json-file',
        `${file}/${service} must use the json-file log driver`,
      );
      assert.equal(
        definition.logging?.options?.['max-size'],
        '50m',
        `${file}/${service} must cap log size at 50m`,
      );
      assert.equal(
        definition.logging?.options?.['max-file'],
        '3',
        `${file}/${service} must rotate at most 3 log files`,
      );
      assert.ok(definition.restart, `${file}/${service} must declare a restart policy`);
      assert.ok(definition.healthcheck, `${file}/${service} must declare a healthcheck`);
    }
  }
});

test('deployment environment registry is the single source of truth for packaging and deploy scripts', () => {
  // The shell deploy script and the per-environment packager must both derive
  // their environment set from scripts/deployment-environments.mjs instead of
  // re-declaring it.
  const deployScript = readFileSync(
    path.join(appRoot, 'scripts', 'deploy-docker-environment.sh'),
    'utf8',
  );
  assert.match(
    deployScript,
    /deployment-environments\.mjs list/u,
    'deploy-docker-environment.sh must read the canonical environment list',
  );
  assert.doesNotMatch(
    deployScript,
    /development\|test\|staging\|demo\|production\)\s*;;/u,
    'deploy-docker-environment.sh must not hardcode the environment case list',
  );

  const shim = readFileSync(
    path.join(appRoot, 'scripts', 'package-docker-environments.sh'),
    'utf8',
  );
  assert.match(
    shim,
    /package-environment-bundles\.mjs/u,
    'the shell packaging shim must delegate to the shared mjs encapsulation',
  );

  const packager = readFileSync(
    path.join(appRoot, 'scripts', 'docker', 'package-environment-bundles.mjs'),
    'utf8',
  );
  assert.match(packager, /from '\.\.\/deployment-environments\.mjs'/u);
  // Deliverable bundles must be built from the committed example templates;
  // requiring the operator's local secret .env would leak secrets into a
  // distributable package (SUPPLY_CHAIN_SECURITY_SPEC).
  assert.match(
    packager,
    /docker', 'env', `\$\{environment\}\.env\.example`/u,
    'the per-environment packager must read the committed .env.example templates',
  );
  assert.doesNotMatch(
    packager,
    /requireRepoFile\(path\.join\('docker', 'env', `\$\{environment\}\.env`\)\)/u,
  );

  const unifiedPackager = readFileSync(
    path.join(appRoot, 'scripts', 'docker', 'package-install-bundle.mjs'),
    'utf8',
  );
  assert.match(
    unifiedPackager,
    /from '\.\.\/deployment-environments\.mjs'/u,
    'the unified bundle packager must import the shared environment registry',
  );
});

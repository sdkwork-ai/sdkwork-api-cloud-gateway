import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const component = readJson(path.join(root, 'specs/component.spec.json'));
const workspaceCargo = read(path.join(root, 'Cargo.toml'));
const gatewayCargo = read(path.join(root, 'crates/sdkwork-api-cloud-gateway/Cargo.toml'));
const databaseBootstrap = read(
  path.join(root, 'crates/sdkwork-api-cloud-gateway/src/embedded_database_bootstrap.rs'),
);
const dependencyBootstrap = read(
  path.join(root, 'crates/sdkwork-api-cloud-gateway/src/embedded_dependency_routes.rs'),
);
const runtimeSource = read(
  path.join(root, 'crates/sdkwork-api-cloud-gateway/src/runtime.rs'),
);

assert.doesNotMatch(
  gatewayCargo,
  /^sdkwork-[a-z0-9-]+-database-host\s*=/mu,
  'platform cloud gateway must not depend directly on owner database-host crates',
);
assert.doesNotMatch(
  gatewayCargo,
  /"dep:sdkwork-[a-z0-9-]+-database-host"/u,
  'foundation features must reach module database lifecycle through owner assemblies',
);
assert.doesNotMatch(
  workspaceCargo,
  /^sdkwork-[a-z0-9-]+-database-host\s*=\s*\{[^\n]*sdkwork-/mu,
  'gateway workspace must not catalog dependency-owned database-host crates',
);
assert.doesNotMatch(
  databaseBootstrap,
  /sdkwork_[a-z0-9_]+_database_host::/u,
  'process pool bootstrap must not call dependency-owned database hosts',
);
assert.match(
  databaseBootstrap,
  /bootstrap_embedded_dependency_contributions\(/u,
  'explicit migration must execute the same owner assembly entrypoints as runtime composition',
);

const integrations = uniqueIntegrations(component.dependencies ?? []);
for (const integration of integrations) {
  const feature = featureBody(gatewayCargo, integration.cargoFeature);
  assert.match(
    feature,
    new RegExp(`"dep:${escapeRegExp(integration.cargoDependency)}"`, 'u'),
    `${integration.cargoFeature} must compile owner assembly ${integration.cargoDependency}`,
  );
  const assemblyDependencies = [...feature.matchAll(/"dep:(sdkwork-api-[a-z0-9-]+-assembly)"/gu)]
    .map((match) => match[1]);
  assert.deepEqual(
    assemblyDependencies,
    [integration.cargoDependency],
    `${integration.cargoFeature} must have exactly one owner API assembly entrypoint`,
  );
  assert.ok(
    dependencyBootstrap.includes(integration.executableExport.split('::')[0]),
    `${integration.workspace} must be initialized through ${integration.executableExport}`,
  );
}

assert.ok(
  runtimeSource.indexOf('bootstrap_embedded_databases')
    < runtimeSource.indexOf('bootstrap_embedded_dependency_contributions'),
  'the process-shared pool must exist before owner assembly initialization',
);

console.log(
  `sdkwork-api-cloud-gateway owner assembly lifecycle standard passed for ${integrations.length} foundation features.`,
);

function uniqueIntegrations(surfaces) {
  const byFeature = new Map();
  for (const surface of surfaces) {
    if (!surface.cargoFeature || !surface.cargoDependency || !surface.executableExport) continue;
    if (!/^sdkwork-api-[a-z0-9-]+-assembly$/u.test(surface.cargoDependency)) continue;
    const current = byFeature.get(surface.cargoFeature);
    if (current) {
      assert.equal(current.cargoDependency, surface.cargoDependency);
      assert.equal(current.executableExport, surface.executableExport);
      continue;
    }
    byFeature.set(surface.cargoFeature, surface);
  }
  return [...byFeature.values()];
}

function featureBody(cargo, feature) {
  const inline = cargo.match(
    new RegExp(`^${escapeRegExp(feature)}\\s*=\\s*\\[([^\\n]*)\\]`, 'mu'),
  );
  if (inline) return inline[1];
  const match = cargo.match(
    new RegExp(`^${escapeRegExp(feature)}\\s*=\\s*\\[(.*?)^\\]`, 'msu'),
  );
  if (match) return match[1];
  assert.fail(`missing Cargo feature ${feature}`);
}

function read(filePath) {
  return fs.readFileSync(filePath, 'utf8');
}

function readJson(filePath) {
  return JSON.parse(read(filePath));
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
}

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const workspaceRoot = path.dirname(root);
const component = readJson(path.join(root, 'specs/component.spec.json'));
const workspaceCargo = read(path.join(root, 'Cargo.toml'));
const gatewayCargo = read(path.join(root, 'crates/sdkwork-api-cloud-gateway/Cargo.toml'));
const bootstrapSource = read(
  path.join(
    root,
    'crates/sdkwork-api-cloud-gateway/src/embedded_database_bootstrap.rs',
  ),
);
const runtimeSource = read(
  path.join(root, 'crates/sdkwork-api-cloud-gateway/src/runtime.rs'),
);

const dependencies = uniqueDependencies([
  ...component.dependencies,
  ...(component.databaseDependencies ?? []),
]);
let previousBootstrapPosition = bootstrapSource.indexOf(
  'let pool = process_database_pool().await?;',
);
assert.notEqual(
  previousBootstrapPosition,
  -1,
  'database bootstrap must create the canonical process pool before module initialization',
);

for (const dependency of dependencies) {
  const repoRoot = path.join(workspaceRoot, dependency.workspace);
  const databaseRoot = path.join(repoRoot, 'database');
  const hostCrates = fs
    .readdirSync(path.join(repoRoot, 'crates'), { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && entry.name.endsWith('-database-host'))
    .map((entry) => entry.name);

  const featureHosts = hostCrates.filter((hostCrate) =>
    featureBody(gatewayCargo, dependency.cargoFeature).includes(
      `"dep:${hostCrate}"`,
    ),
  );
  assert.equal(
    featureHosts.length,
    1,
    `${dependency.workspace} must compile exactly one convention-named *-database-host crate through ${dependency.cargoFeature}`,
  );
  assert.ok(
    fs.existsSync(databaseRoot),
    `${dependency.workspace} must own a database/ lifecycle asset directory`,
  );

  const hostCrate = featureHosts[0];
  if (dependency.bootstrapExport) {
    assert.equal(
      dependency.cargoDependency,
      hostCrate,
      `${dependency.workspace} database dependency must name its executable host crate`,
    );
  }
  assert.match(
    workspaceCargo,
    new RegExp(`^${escapeRegExp(hostCrate)}\\s*=\\s*\\{[^\\n]*path\\s*=`, 'mu'),
    `${hostCrate} must be a gateway workspace dependency`,
  );
  assert.match(
    featureBody(gatewayCargo, dependency.cargoFeature),
    new RegExp(`"dep:${escapeRegExp(hostCrate)}"`, 'u'),
    `${dependency.cargoFeature} must compile ${hostCrate} with its gateway assembly`,
  );
  assert.match(
    gatewayCargo,
    new RegExp(
      `^${escapeRegExp(hostCrate)}\\s*=\\s*\\{\\s*workspace\\s*=\\s*true,\\s*optional\\s*=\\s*true\\s*\\}`,
      'mu',
    ),
    `${hostCrate} must be an optional crate dependency`,
  );
  assert.ok(
    bootstrapSource.includes(hostCrate.replaceAll('-', '_')),
    `${hostCrate} must have an executable startup bootstrap call`,
  );

  const bootstrapPosition = bootstrapSource.indexOf(
    `"${dependency.workspace}"`,
    previousBootstrapPosition + 1,
  );
  assert.notEqual(
    bootstrapPosition,
    -1,
    `${dependency.workspace} must be initialized by the startup orchestrator`,
  );
  assert.ok(
    bootstrapPosition > previousBootstrapPosition,
    `${dependency.workspace} must preserve component.spec.json dependency order`,
  );
  previousBootstrapPosition = bootstrapPosition;

  const manifestPath = path.join(databaseRoot, 'database.manifest.json');
  assert.ok(
    fs.existsSync(manifestPath),
    `${dependency.workspace} must expose database/database.manifest.json`,
  );
  const databaseManifest = readJson(manifestPath);
  assert.equal(
    typeof databaseManifest.moduleId,
    'string',
    `${dependency.workspace} database manifest must declare moduleId`,
  );
  assert.ok(
    bootstrapSource.includes(`"${databaseManifest.moduleId}"`),
    `${dependency.workspace} bootstrap must identify module ${databaseManifest.moduleId}`,
  );
}

assert.match(
  runtimeSource,
  /bootstrap_embedded_databases\(&config\)\.await\?/u,
  'runtime must finish database initialization before dependency router assembly',
);
assert.ok(
  runtimeSource.indexOf('bootstrap_embedded_databases(&config).await?')
    < runtimeSource.indexOf('bootstrap_embedded_dependency_routers(&config).await?'),
  'database initialization must run before dependency router assembly',
);

console.log(
  `sdkwork-api-cloud-gateway database bootstrap standard passed for ${dependencies.length} workspaces.`,
);

function uniqueDependencies(entries) {
  const seen = new Set();
  const result = [];
  for (const entry of entries) {
    if (seen.has(entry.workspace)) {
      continue;
    }
    seen.add(entry.workspace);
    result.push(entry);
  }
  return result;
}

function featureBody(cargo, feature) {
  const match = cargo.match(
    new RegExp(`^${escapeRegExp(feature)}\\s*=\\s*\\[(.*?)^\\]`, 'msu'),
  );
  if (match) {
    return match[1];
  }
  const inline = cargo.match(
    new RegExp(`^${escapeRegExp(feature)}\\s*=\\s*\\[([^\\n]*)\\]`, 'mu'),
  );
  assert.ok(inline, `missing Cargo feature ${feature}`);
  return inline[1];
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

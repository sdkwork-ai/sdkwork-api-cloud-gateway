import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..', '..');

function read(relativePath, root = repoRoot) {
  return fs.readFileSync(path.join(root, relativePath), 'utf8');
}

const runtimeSource = read('crates/sdkwork-api-cloud-gateway/src/runtime.rs');
const databaseBootstrapSource = read(
  'crates/sdkwork-api-cloud-gateway/src/embedded_database_bootstrap.rs',
);
const dependencyRoutesSource = read(
  'crates/sdkwork-api-cloud-gateway/src/embedded_dependency_routes.rs',
);
const embeddedAppbaseBuilderSource = runtimeSource.slice(
  runtimeSource.indexOf('pub async fn build_sdkwork_api_cloud_gateway_router_with_embedded_appbase'),
  runtimeSource.indexOf('async fn metrics_endpoint'),
);
const gatewayCargo = read('crates/sdkwork-api-cloud-gateway/Cargo.toml');

// 1) IAM schema lifecycle must stay behind its owner API assembly.
assert.match(
  dependencyRoutesSource,
  /sdkwork_api_iam_assembly::assemble_owner_api_surfaces_with_pool\(pool\.clone\(\)\)/u,
  'API cloud gateway must initialize IAM through the canonical owner assembly.',
);
assert.doesNotMatch(
  databaseBootstrapSource,
  /sdkwork_iam_database_host|bootstrap_iam_database/u,
  'API cloud gateway must not call the IAM database host outside its owner assembly.',
);

// 3) The embedded builder provisions the process-shared pool and composes the
//    IAM web framework resolver before serving.
assert.match(
  runtimeSource,
  /iam_web_request_context_resolver_from_database_pool_for_audiences\(/u,
  'Embedded IAM assembly build must resolve audience-bound request context from the process pool.',
);
assert.match(
  runtimeSource,
  /bootstrap_embedded_databases\(&config\)\s*\.await\?/u,
  'API cloud gateway must bootstrap the process-shared database pool.',
);

// 4) Cargo wiring: canonical assemblies, no direct IAM route crates.
assert.match(
  gatewayCargo,
  /sdkwork-api-iam-assembly/u,
  'API cloud gateway must depend on sdkwork-api-iam-assembly.',
);
assert.match(
  gatewayCargo,
  /sdkwork-iam-web-adapter/u,
  'API cloud gateway must depend on sdkwork-iam-web-adapter.',
);
assert.doesNotMatch(
  gatewayCargo,
  /sdkwork-iam-database-host/u,
  'API cloud gateway must not depend directly on sdkwork-iam-database-host.',
);
assert.doesNotMatch(
  gatewayCargo,
  /sdkwork_routes_iam_(?:app|backend)_api/u,
  'API cloud gateway must not depend directly on IAM route crates.',
);

console.log('sdkwork-api-cloud-gateway IAM application bootstrap standard passed.');

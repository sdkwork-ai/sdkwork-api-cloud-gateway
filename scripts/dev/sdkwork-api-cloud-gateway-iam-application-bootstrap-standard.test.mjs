import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..', '..');
const iamRepoRoot = path.resolve(repoRoot, '..', 'sdkwork-iam');

function read(relativePath, root = repoRoot) {
  return fs.readFileSync(path.join(root, relativePath), 'utf8');
}

const bootstrapSource = read('crates/sdkwork-api-cloud-gateway/src/iam_application_bootstrap.rs');
const runtimeSource = read('crates/sdkwork-api-cloud-gateway/src/runtime.rs');
const backendRouterBuilderSource = runtimeSource.slice(
  runtimeSource.indexOf('pub async fn build_embedded_sdkwork_iam_backend_api_router'),
  runtimeSource.indexOf('pub async fn build_embedded_sdkwork_iam_app_api_router'),
);
const gatewayCargo = read('crates/sdkwork-api-cloud-gateway/Cargo.toml');
const sharedBootstrapSource = read(
  'crates/sdkwork-iam-embedded-application-bootstrap/src/runtime.rs',
  iamRepoRoot,
);

assert.match(
  bootstrapSource,
  /ensure_tenant_application_from_app_root_with_env_and_fallback/u,
  'API cloud gateway must provision tenant applications with repository-root fallback.',
);

assert.match(
  bootstrapSource,
  /bootstrap_iam_database_from_env/u,
  'API cloud gateway must bootstrap IAM schema before tenant application provisioning.',
);

assert.match(
  runtimeSource,
  /ensure_embedded_iam_tenant_application_bootstrap/u,
  'Embedded IAM router build must provision tenant applications before credential-entry routes go live.',
);

assert.match(
  backendRouterBuilderSource,
  /ensure_embedded_iam_router_prerequisites/u,
  'Embedded IAM backend-only startup must fail closed when database lifecycle bootstrap fails.',
);

assert.match(
  gatewayCargo,
  /sdkwork-iam-embedded-application-bootstrap/u,
  'API cloud gateway must depend on sdkwork-iam-embedded-application-bootstrap.',
);

assert.match(
  gatewayCargo,
  /sdkwork-iam-database-host/u,
  'API cloud gateway must depend on sdkwork-iam-database-host.',
);

assert.match(
  sharedBootstrapSource,
  /postgres_url_with_search_path/u,
  'Shared embedded bootstrap must align postgres search_path before provisioning.',
);

console.log('sdkwork-api-cloud-gateway IAM application bootstrap standard passed.');

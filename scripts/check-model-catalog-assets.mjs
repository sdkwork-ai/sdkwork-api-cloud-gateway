#!/usr/bin/env node
/**
 * Release-blocking check for the official models catalog bundle.
 *
 * A greenfield gateway seeds `ai_model_vendor` from
 * data/sdkwork-models at first boot. If a release package ships a partial or
 * empty catalog root, startup succeeds, /readyz is healthy, and every
 * model-vendor / resource-group / model-mapping admin page stays blank.
 * This gate keeps the asset list, packaging scripts, and runtime environment
 * wiring aligned so that failure mode cannot ship silently.
 *
 * Exit code 0 = pass, 1 = violations.
 */

import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import {
  MODELS_CATALOG_ASSETS,
  MODELS_CATALOG_INSTALL_DIR,
  MODELS_CATALOG_REQUIRED_FILES,
  missingModelsCatalogAssets,
  resolveModelsCatalogSourceRoot,
} from './package-server.mjs';

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const failures = [];

const catalogRoot = resolveModelsCatalogSourceRoot();
const missing = missingModelsCatalogAssets(catalogRoot);
if (missing.length > 0) {
  failures.push(`official catalog bundle at ${catalogRoot} is missing: ${missing.join(', ')}`);
}

// Both packaging pipelines must consume the single authoritative asset list
// and fail closed; a locally duplicated (or silently skipped) copy list is how
// an empty catalog root reaches production.
for (const script of [
  'scripts/package-server.mjs',
  'scripts/build-api-cloud-gateway-container.mjs',
]) {
  const scriptPath = path.join(appRoot, script);
  if (!existsSync(scriptPath)) {
    failures.push(`missing packaging script ${script}`);
    continue;
  }
  const source = readFileSync(scriptPath, 'utf8');
  for (const symbol of [
    'MODELS_CATALOG_ASSETS',
    'MODELS_CATALOG_REQUIRED_FILES',
    'MODELS_CATALOG_INSTALL_DIR',
  ]) {
    if (!source.includes(symbol)) {
      failures.push(`${script} does not use the shared ${symbol} list`);
    }
  }
  if (
    !source.includes('assertModelsCatalogSource')
    && !source.includes('missingModelsCatalogAssets')
  ) {
    failures.push(`${script} does not fail closed on an incomplete catalog bundle`);
  }
}

// Container image and standalone installer must both point the seed loader at
// the packaged root. A missing env var falls back to a compile-time source path
// that never exists on a customer machine.
for (const declaration of ['Dockerfile', 'bin/packaging/install-ubuntu.sh']) {
  const declarationPath = path.join(appRoot, declaration);
  if (!existsSync(declarationPath)) {
    failures.push(`missing runtime declaration file ${declaration}`);
    continue;
  }
  if (!readFileSync(declarationPath, 'utf8').includes('SDKWORK_MODELS_CATALOG_ROOT')) {
    failures.push(`${declaration} does not declare SDKWORK_MODELS_CATALOG_ROOT`);
  }
}

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(`[model-catalog-assets] FAIL ${failure}`);
  }
  process.exit(1);
}

console.log(
  `[model-catalog-assets] PASS: ${MODELS_CATALOG_ASSETS.length} packaged roots, `
  + `${MODELS_CATALOG_REQUIRED_FILES.length} loader-required files, `
  + `install dir ${MODELS_CATALOG_INSTALL_DIR}`,
);

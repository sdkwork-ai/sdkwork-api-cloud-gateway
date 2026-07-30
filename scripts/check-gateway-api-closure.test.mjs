import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  routesFromRustManifest,
  validateGatewayApiClosure,
} from "./check-gateway-api-closure.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

test("embedded gateway dependency APIs form a complete collision-free route closure", () => {
  const result = validateGatewayApiClosure(root);
  assert.deepEqual(result.errors, []);
  assert.ok(result.routes.length > 0);
});

test("Rust route manifests resolve constants from imported sibling modules", () => {
  const fixtureRoot = fs.mkdtempSync(
    path.join(os.tmpdir(), "sdkwork-gateway-rust-manifest-"),
  );
  try {
    const manifestPath = path.join(fixtureRoot, "manifest.rs");
    fs.writeFileSync(
      path.join(fixtureRoot, "paths.rs"),
      'pub const HEALTH: &str = "/backend/v3/api/ops/health";\n',
    );
    fs.writeFileSync(
      manifestPath,
      [
        "use crate::paths;",
        'const LOCAL: &str = "/backend/v3/api/ops/local";',
        "const ROUTES: &[HttpRoute] = &[",
        '  HttpRoute::dual_token(HttpMethod::Get, paths::HEALTH, "ops", "ops.health.retrieve"),',
        '  HttpRoute::dual_token(HttpMethod::Post, LOCAL, "ops", "ops.local.create"),',
        "];",
      ].join("\n"),
    );

    assert.deepEqual(routesFromRustManifest(manifestPath), [
      {
        method: "GET",
        path: "/backend/v3/api/ops/health",
        operationId: undefined,
        apiAuthority: undefined,
      },
      {
        method: "POST",
        path: "/backend/v3/api/ops/local",
        operationId: undefined,
        apiAuthority: undefined,
      },
    ]);
  } finally {
    fs.rmSync(fixtureRoot, { recursive: true, force: true });
  }
});

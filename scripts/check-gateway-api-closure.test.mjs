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
  assert.ok(
    result.routes.some(
      (route) =>
        route.serviceId === "sdkwork-shop-backend-api" &&
        route.path === "/backend/v3/api/catalog/categories" &&
        route.source.startsWith("sdkwork-merchandise/"),
    ),
    "nested merchandise assembly routes must remain attributed to their provider workspace",
  );
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

test("RouteDefinition projections resolve method/path from the sibling routes module", () => {
  const fixtureRoot = fs.mkdtempSync(
    path.join(os.tmpdir(), "sdkwork-gateway-route-definition-"),
  );
  try {
    // The manifest projection only references `route_definitions()`; the
    // literals live in the sibling module (sdkwork route generator output).
    const manifestPath = path.join(fixtureRoot, "manifest.rs");
    fs.writeFileSync(manifestPath, [
      "use crate::paths::{API_AUTHORITY, PREFIX, SURFACE};",
      "use crate::routes::{route_definitions, RouteDefinition};",
      "",
      "pub fn route_manifest() -> RouteManifest {",
      "    RouteManifest { routes: route_definitions() }",
      "}",
    ].join("\n"));
    fs.writeFileSync(
      path.join(fixtureRoot, "routes.rs"),
      [
        "pub const ROUTES: &[RouteDefinition] = &[",
        "    RouteDefinition {",
        '        method: "GET",',
        '        path: "/store/v3/api/user-stores/{shareToken}",',
        '        operation_id: "appstore.userStores.public.retrieve",',
        '        auth: RouteAuth::Public,',
        "    },",
        "    RouteDefinition {",
        '        method: "POST",',
        '        path: "/store/v3/api/user-stores",',
        '        operation_id: "appstore.userStores.create",',
        '        auth: RouteAuth::Public,',
        "    },",
        "];",
      ].join("\n"),
    );

    assert.deepEqual(routesFromRustManifest(manifestPath), [
      {
        method: "GET",
        path: "/store/v3/api/user-stores/{shareToken}",
        operationId: "appstore.userStores.public.retrieve",
        apiAuthority: undefined,
      },
      {
        method: "POST",
        path: "/store/v3/api/user-stores",
        operationId: "appstore.userStores.create",
        apiAuthority: undefined,
      },
    ]);
  } finally {
    fs.rmSync(fixtureRoot, { recursive: true, force: true });
  }
});

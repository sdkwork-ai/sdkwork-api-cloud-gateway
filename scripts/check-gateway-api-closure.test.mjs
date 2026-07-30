import assert from "node:assert/strict";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { validateGatewayApiClosure } from "./check-gateway-api-closure.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

test("embedded gateway dependency APIs form a complete collision-free route closure", () => {
  const result = validateGatewayApiClosure(root);
  assert.deepEqual(result.errors, []);
  assert.ok(result.routes.length > 0);
});

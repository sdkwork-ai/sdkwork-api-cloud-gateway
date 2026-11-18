#!/usr/bin/env node
/**
 * OpenAPI auth-contract conformance for this module.
 *
 * The spec-gate family checks operation patterns, envelopes, pagination and the
 * int64 wire contract, but nothing checked the rule that made the production
 * `/v1/chat/completions` deployment fail:
 *
 *   API_SPEC §904  `security` and `x-sdkwork-auth-mode` MUST agree.
 *   API_SPEC §4.5.2 vendor-compatibility operations MUST declare both
 *                  `x-sdkwork-wire-protocol: external` and
 *                  `x-sdkwork-external-protocol-id` on the same operation.
 *   API_SPEC §365  the legacy route-manifest value `public` MUST be normalized
 *                  to `anonymous` before validation or output.
 *
 * `x-sdkwork-auth-mode` is what the route manifest materials and the runtime
 * enforce; `security` is what clients and generated SDKs read. When they
 * disagree, a correctly credentialed caller is rejected by the runtime guard
 * even though the published contract promised that credential.
 *
 * Usage:
 *   node scripts/check-openapi-auth-conformance.mjs --root .
 *   node scripts/check-openapi-auth-conformance.mjs --root . --report-workspace ..
 *
 * `--report-workspace` audits sibling repositories too and prints their debt,
 * but never changes this module's exit status: a module can only be held to the
 * contracts it owns.
 */
import fs from 'node:fs';
import path from 'node:path';
import YAML from 'yaml';

const args = process.argv.slice(2);
function argValue(name, fallback) {
  const index = args.indexOf(name);
  return index >= 0 && args[index + 1] ? args[index + 1] : fallback;
}

const root = path.resolve(argValue('--root', '.'));
const reportWorkspace = argValue('--report-workspace', null);

const IGNORE_DIRS = new Set([
  'node_modules',
  '.git',
  'target',
  'dist',
  'build',
  '.runtime',
  '.tmp',
  'coverage',
  'artifacts',
  'generated',
  'external',
  'docs',
]);

/** Expected `security` alternatives for each canonical auth mode (API_SPEC §9.2). */
const MODE_SECURITY = {
  anonymous: [[]],
  'api-key': [[['ApiKey']]],
  oauth: [[['OAuthBearer']], [['bearerAuth']]],
  'open-api-flexible': [
    [['ApiKey'], ['OAuthBearer']],
    [['OAuthBearer'], ['ApiKey']],
  ],
  'api-key-or-dual-token': [
    [['ApiKey'], ['AccessToken', 'AuthToken']],
    [['AccessToken', 'AuthToken'], ['ApiKey']],
    [['ApiKey'], ['AuthToken', 'AccessToken']],
    [['AuthToken', 'AccessToken'], ['ApiKey']],
  ],
  'dual-token': [[['AccessToken', 'AuthToken']], [['AuthToken', 'AccessToken']]],
  'dual-token-or-anonymous': [
    [['AccessToken', 'AuthToken']],
    [['AuthToken', 'AccessToken']],
    [],
  ],
  'credential-entry-bootstrap': [[['AccessToken']]],
  'refresh-token': [[]],
  'bootstrap-body': [[]],
  'ingress-token': [
    [['IngressToken', 'AccessToken']],
    [['AccessToken', 'IngressToken']],
  ],
  'agent-token': [
    [['AgentToken', 'AccessToken']],
    [['AccessToken', 'AgentToken']],
  ],
  // Operation-defined external protocol: `security` is the upstream's own shape.
  compatibility: null,
};

const LEGACY_MODES = new Set(['public']);
const HTTP_METHODS = new Set(['get', 'put', 'post', 'delete', 'patch', 'head', 'options']);

function walk(dir, acc = []) {
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return acc;
  }
  for (const entry of entries) {
    if (IGNORE_DIRS.has(entry.name)) continue;
    const full = path.join(dir, entry.name);
    let stat;
    try {
      stat = fs.lstatSync(full);
    } catch {
      continue;
    }
    if (stat.isSymbolicLink()) continue;
    if (stat.isDirectory()) walk(full, acc);
    else if (/openapi\.(json|ya?ml)$/i.test(entry.name)) acc.push(full);
  }
  return acc;
}

function readDocument(file) {
  const text = fs.readFileSync(file, 'utf8');
  return /\.ya?ml$/i.test(file) ? YAML.parse(text) : JSON.parse(text);
}

const securityShape = (security) =>
  Array.isArray(security) ? security.map((entry) => Object.keys(entry ?? {}).sort()) : null;

const sameShape = (left, right) =>
  left.length === right.length && left.every((row, i) => row.join(',') === right[i].join(','));

/**
 * Audits one parsed OpenAPI document. Pure: the file path is only a label.
 * Exported so `--self-test` can prove each rule fires without touching disk.
 */
export function auditDocument(document, rel) {
  const findings = [];
  if (!document || typeof document !== 'object' || !document.paths) return findings;
  const schemes = Object.keys(document.components?.securitySchemes ?? {});

  for (const [pathKey, pathItem] of Object.entries(document.paths)) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (!HTTP_METHODS.has(method)) continue;
      if (!operation || typeof operation !== 'object') continue;
      const where = `${rel}: ${method.toUpperCase()} ${pathKey}`;
      const mode = operation['x-sdkwork-auth-mode'];
      const security = operation.security;
      const shape = securityShape(security);
      const external = operation['x-sdkwork-wire-protocol'] === 'external';
      const externalId = operation['x-sdkwork-external-protocol-id'];

      if (external && !externalId) {
        findings.push(
          `${where}: x-sdkwork-wire-protocol: external requires x-sdkwork-external-protocol-id (API_SPEC §4.5.2)`,
        );
      }
      if (!external && externalId) {
        findings.push(
          `${where}: x-sdkwork-external-protocol-id requires x-sdkwork-wire-protocol: external (API_SPEC §4.5.2)`,
        );
      }
      for (const name of (shape ?? []).flat()) {
        if (schemes.length > 0 && !schemes.includes(name)) {
          findings.push(
            `${where}: security references undeclared scheme "${name}" (components.securitySchemes)`,
          );
        }
      }
      if (typeof mode !== 'string') continue;
      if (LEGACY_MODES.has(mode)) {
        findings.push(
          `${where}: legacy auth-mode "${mode}" must be normalized to "anonymous" (API_SPEC §365)`,
        );
        continue;
      }
      const expected = MODE_SECURITY[mode];
      if (expected === undefined) {
        findings.push(`${where}: unknown x-sdkwork-auth-mode "${mode}" (API_SPEC §365)`);
        continue;
      }
      if (expected === null) continue;
      if (!shape) {
        findings.push(
          `${where}: auth-mode "${mode}" declares no security object (API_SPEC §904)`,
        );
        continue;
      }
      if (!expected.some((candidate) => sameShape(shape, candidate))) {
        findings.push(
          `${where}: security ${JSON.stringify(security)} does not express auth-mode "${mode}" (API_SPEC §904)`,
        );
      }
    }
  }
  return findings;
}

function auditFile(file, displayRoot) {
  const rel = relative(file, displayRoot);
  let document;
  try {
    document = readDocument(file);
  } catch (error) {
    return [`${rel}: unreadable document: ${error.message}`];
  }
  return auditDocument(document, rel);
}

/** Fixtures that pin each rule: (name, document, expectedFindings). */
const SELF_TEST_CASES = [
  {
    name: 'matching api-key profile passes',
    document: doc({
      security: [{ ApiKey: [] }],
      'x-sdkwork-auth-mode': 'api-key',
    }),
    findings: 0,
  },
  {
    name: 'bearer-only security with api-key-or-dual-token is rejected (§904)',
    document: doc({
      security: [{ bearerAuth: [] }],
      'x-sdkwork-auth-mode': 'api-key-or-dual-token',
    }),
    findings: 1,
  },
  {
    name: 'complete dual-token pair passes',
    document: doc({
      security: [{ ApiKey: [] }, { AuthToken: [], AccessToken: [] }],
      'x-sdkwork-auth-mode': 'api-key-or-dual-token',
    }),
    findings: 0,
  },
  {
    name: 'legacy public is rejected (§365)',
    document: doc({ security: [], 'x-sdkwork-auth-mode': 'public' }),
    findings: 1,
  },
  {
    name: 'external marker without protocol id is rejected (§4.5.2)',
    document: doc({
      security: [{ ApiKey: [] }],
      'x-sdkwork-auth-mode': 'api-key',
      'x-sdkwork-wire-protocol': 'external',
    }),
    findings: 1,
  },
  {
    name: 'compatibility keeps the operation-defined security shape',
    document: doc({
      security: [{ bearerAuth: [] }],
      'x-sdkwork-auth-mode': 'compatibility',
      'x-sdkwork-wire-protocol': 'external',
      'x-sdkwork-external-protocol-id': 'openai-v1',
    }),
    findings: 0,
  },
  {
    name: 'security referencing an undeclared scheme is rejected',
    document: doc({ security: [{ Missing: [] }], 'x-sdkwork-auth-mode': 'api-key' }),
    findings: 2,
  },
];

function doc(operation) {
  return {
    openapi: '3.1.2',
    components: {
      securitySchemes: {
        ApiKey: { type: 'apiKey', in: 'header', name: 'X-API-Key' },
        AuthToken: { type: 'http', scheme: 'bearer' },
        AccessToken: { type: 'apiKey', in: 'header', name: 'Access-Token' },
        bearerAuth: { type: 'http', scheme: 'bearer' },
      },
    },
    paths: { '/probe': { get: operation } },
  };
}

function runSelfTest() {
  let failures = 0;
  for (const testCase of SELF_TEST_CASES) {
    const actual = auditDocument(testCase.document, 'fixture').length;
    if (actual === testCase.findings) {
      console.log(`PASS  ${testCase.name}`);
    } else {
      failures += 1;
      console.error(
        `FAIL  ${testCase.name} (expected ${testCase.findings} finding(s), got ${actual})`,
      );
    }
  }
  if (failures > 0) {
    console.error(`openapi auth conformance self-test: ${failures} failure(s)`);
    process.exitCode = 1;
    return;
  }
  console.log(`openapi auth conformance self-test: ${SELF_TEST_CASES.length} cases passed`);
}

function relative(file, displayRoot) {
  return path.relative(displayRoot, file).replace(/\\/g, '/');
}

if (args.includes('--self-test')) {
  runSelfTest();
  process.exit(process.exitCode ?? 0);
}

const ownFindings = [];
for (const file of walk(root)) {
  ownFindings.push(...auditFile(file, root));
}

if (ownFindings.length > 0) {
  console.error(`openapi auth conformance: ${ownFindings.length} violation(s)`);
  for (const finding of ownFindings) console.error(`  - ${finding}`);
  process.exitCode = 1;
} else {
  console.log('openapi auth conformance: passed');
}

if (reportWorkspace) {
  const workspaceRoot = path.resolve(reportWorkspace);
  const debt = new Map();
  for (const file of walk(workspaceRoot)) {
    const repo = (relative(file, workspaceRoot).match(/^sdkwork-[^/]+/) ?? ['?'])[0];
    const count = auditFile(file, workspaceRoot).length;
    if (count > 0) debt.set(repo, (debt.get(repo) ?? 0) + count);
  }
  if (debt.size > 0) {
    console.error('openapi auth conformance (workspace debt, not scored):');
    for (const [repo, count] of [...debt.entries()].sort()) {
      console.error(`  - ${repo}: ${count}`);
    }
  }
}

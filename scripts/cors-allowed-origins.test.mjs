#!/usr/bin/env node
/**
 * Regression tests for the canonical browser CORS configuration.
 *
 * Guards the invariants the compact (pattern) form depends on:
 *   - the label registry stays a superset of the previously materialized exact
 *     allow-list, so swapping enumeration for the pattern cannot drop a console;
 *   - the pattern still expresses the full
 *     `label x base domain x environment x scheme` closure;
 *   - production stays HTTPS-only while non-production keeps HTTP;
 *   - the edge self-origin (`api`), the webserver consoles (`server*`) and every
 *     module console family stay covered;
 *   - port-bearing, nested-host and loopback origins behave as the framework
 *     matches them.
 */

import assert from 'node:assert/strict';
import test from 'node:test';

import { ENVIRONMENTS } from './deployment-environments.mjs';
import {
  BASE_DOMAINS,
  canonicalConsoleHostConfig,
  canonicalConsoleOrigins,
  canonicalSeedOrigins,
  CONSOLE_ENV_SCHEMES,
  CONSOLE_ENV_SUFFIX,
  consoleHostLabelRegistry,
  isCanonicalConsoleOrigin,
  isConsoleHostForEnvironment,
  isManagedConsoleOrigin,
  isPatternCoveredOrigin,
  isRegisteredClientOrigin,
  WEB_CONSOLE_CLIENT_ROLES,
} from './cors-allowed-origins.mjs';

/**
 * Labels the previously materialized (and five-environment verified) exact
 * allow-list carried. The pattern must remain a superset of these: if one is
 * dropped, the compact form is a silent CORS regression.
 */
const PREVIOUSLY_MATERIALIZED_LABELS = [
  'account',
  'admin',
  'agents',
  'agentstudio',
  'api',
  'appstore',
  'assets',
  'cloudrouter',
  'cms',
  'code',
  'community',
  'drive',
  'iam',
  'im',
  'portal',
  'router',
  'router-admin',
  'router-open',
  'settings',
];

test('label registry is a superset of the previously materialized allow-list', () => {
  const registry = new Set(consoleHostLabelRegistry());
  for (const label of PREVIOUSLY_MATERIALIZED_LABELS) {
    assert.ok(registry.has(label), `label ${label} was dropped by the compact form`);
  }
  assert.ok(
    registry.size >= PREVIOUSLY_MATERIALIZED_LABELS.length,
    'registry shrank below the enumerated allow-list',
  );
});

test('every registered label is a single lowercase DNS label and unique', () => {
  const labels = consoleHostLabelRegistry();
  assert.equal(new Set(WEB_CONSOLE_CLIENT_ROLES).size, WEB_CONSOLE_CLIENT_ROLES.length);
  for (const label of labels) {
    assert.match(label, /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/, `not a DNS label: ${label}`);
    assert.ok(!label.includes('.'), `labels must not nest: ${label}`);
  }
});

test('the pattern expresses the full label x base-domain x scheme closure', () => {
  for (const environment of ENVIRONMENTS) {
    const origins = canonicalConsoleOrigins(environment);
    const expected =
      consoleHostLabelRegistry().length *
      BASE_DOMAINS.length *
      CONSOLE_ENV_SCHEMES[environment].length;
    assert.equal(origins.length, expected, `${environment}: closure size`);
    assert.equal(new Set(origins).size, origins.length, `${environment}: deduplicated`);
    for (const origin of origins) {
      assert.ok(isCanonicalConsoleOrigin(origin, environment), `${environment}: ${origin}`);
      assert.ok(isPatternCoveredOrigin(origin, environment), `${environment}: ${origin}`);
    }
  }
});

test('the reported preflight failure is covered in every environment', () => {
  // http://api-dev.sdkwork.com/... called from http://im-dev.sdkwork.com was
  // blocked because the carrier omitted the origin.
  assert.ok(isPatternCoveredOrigin('http://im-dev.sdkwork.com', 'development'));
  assert.ok(isPatternCoveredOrigin('http://im-test.sdkwork.com', 'test'));
  assert.ok(isPatternCoveredOrigin('http://im-staging.sdkwork.com', 'staging'));
  assert.ok(isPatternCoveredOrigin('http://im-demo.sdkwork.com', 'demo'));
  assert.ok(isPatternCoveredOrigin('https://im.sdkwork.com', 'production'));
});

test('the edge self-origin and the bundled webserver consoles stay covered', () => {
  // `api` is a registered label: the development PC console is served from the
  // API plane host, and that origin was in the previously verified allow-list.
  assert.ok(isPatternCoveredOrigin('http://api-dev.sdkwork.com', 'development'));
  assert.ok(isPatternCoveredOrigin('https://api.sdkwork.com', 'production'));
  for (const label of ['server', 'server-admin', 'server-app']) {
    assert.ok(isPatternCoveredOrigin(`http://${label}-dev.sdkwork.com`, 'development'), label);
    assert.ok(isPatternCoveredOrigin(`https://${label}.sdkwork.com`, 'production'), label);
  }
});

test('production is HTTPS-only while non-production keeps HTTP', () => {
  assert.ok(!isPatternCoveredOrigin('http://im.sdkwork.com', 'production'));
  assert.ok(isPatternCoveredOrigin('https://im.sdkwork.com', 'production'));
  assert.ok(isPatternCoveredOrigin('http://im-dev.sdkwork.com', 'development'));
  assert.ok(isPatternCoveredOrigin('https://im-dev.sdkwork.com', 'development'));
  assert.ok(!isPatternCoveredOrigin('http://im-dev.sdkwork.com', 'production'));
  assert.ok(!isPatternCoveredOrigin('https://im-dev.sdkwork.com', 'production'));
});

test('the pattern is exact: ports yes, nesting no', () => {
  // A dev console served on a non-standard port is still the same host.
  assert.ok(isPatternCoveredOrigin('http://im-dev.sdkwork.com:13800', 'development'));
  // Prefix matching must never swallow a nested host.
  assert.ok(!isPatternCoveredOrigin('http://router.open.sdkwork.com', 'production'));
  assert.ok(!isPatternCoveredOrigin('http://im-dev.sdkwork.com.attacker.example', 'development'));
  assert.ok(!isPatternCoveredOrigin('http://notregistered.example.com', 'development'));
  assert.ok(!isPatternCoveredOrigin('http://user@im-dev.sdkwork.com', 'development'));
  assert.ok(!isPatternCoveredOrigin('ftp://im-dev.sdkwork.com', 'development'));
  assert.ok(!isPatternCoveredOrigin('null', 'development'));
  assert.ok(!isPatternCoveredOrigin('app://dsh', 'development'));
});

test('loopback seeds are exact-only and never pattern-covered', () => {
  for (const environment of ENVIRONMENTS) {
    const seeds = canonicalSeedOrigins(environment);
    assert.equal(seeds.length, 2, `${environment}: loopback seeds`);
    for (const seed of seeds) {
      assert.match(seed, /^http:\/\/(localhost|127\.0\.0\.1):\d+$/u, seed);
      assert.ok(!isPatternCoveredOrigin(seed, environment), `${environment}: ${seed}`);
    }
  }
});

test('registered client origins are merged by the framework, not listed', () => {
  for (const origin of ['app://dsh', 'tauri://localhost', 'https://servicewechat.com']) {
    assert.ok(isRegisteredClientOrigin(origin), origin);
    assert.ok(!isManagedConsoleOrigin(origin), origin);
  }
});

test('managed-console classifier tracks the registry', () => {
  assert.ok(isManagedConsoleOrigin('http://im-dev.sdkwork.com'));
  assert.ok(isManagedConsoleOrigin('http://router-admin-dev.sdkwork.com'));
  assert.ok(isManagedConsoleOrigin('https://account.sdkwork.com'));
  assert.ok(isManagedConsoleOrigin('http://cloudrouter-dev.sdkwork.com'));
  assert.ok(isManagedConsoleOrigin('http://server-dev.sdkwork.com'));
  // The API plane host is registered now that `api` is a label.
  assert.ok(isManagedConsoleOrigin('http://api-dev.sdkwork.com'));
  // Dead placeholders and non-console origins never became labels.
  assert.ok(!isManagedConsoleOrigin('http://router-admin-{s}.sdkwork.com'));
  assert.ok(!isManagedConsoleOrigin('http://localhost:3910'));
  assert.ok(!isManagedConsoleOrigin('http://router.open.sdkwork.com'));
});

test('environment suffix and scheme tables match the deployment registry', () => {
  for (const environment of ENVIRONMENTS) {
    assert.ok(CONSOLE_ENV_SUFFIX[environment] !== undefined, `${environment}: suffix`);
    assert.ok(CONSOLE_ENV_SCHEMES[environment]?.length > 0, `${environment}: schemes`);
    const config = canonicalConsoleHostConfig(environment);
    assert.equal(config.suffix, CONSOLE_ENV_SUFFIX[environment]);
    assert.equal(config.labels.split(',').length, consoleHostLabelRegistry().length);
    assert.ok(
      isConsoleHostForEnvironment(
        `im${CONSOLE_ENV_SUFFIX[environment]}.sdkwork.com`,
        environment,
      ),
    );
  }
});

test('base domain registry mirrors the platform base-domain family', () => {
  assert.equal(BASE_DOMAINS.length, 16);
  assert.equal(new Set(BASE_DOMAINS).size, BASE_DOMAINS.length);
  assert.ok(BASE_DOMAINS.includes('sdkwork.com'));
  assert.ok(BASE_DOMAINS.includes('86offer.cn'));
});

#!/usr/bin/env node
/**
 * Canonical browser CORS configuration for the SDKWork platform API edge.
 *
 * Single source of truth for the two halves of the edge CORS contract:
 *
 *   1. the **registered console host pattern**
 *        `<scheme>://<label><environment-suffix>.<registered base domain>`
 *      materialized as SDKWORK_CORS_CONSOLE_HOST_{LABELS,SUFFIX,SCHEMES}; and
 *   2. the small **exact origin list** (loopback seeds and any legacy surface the
 *      pattern cannot express) materialized as SDKWORK_CORS_ALLOWED_ORIGINS.
 *
 * Why the pattern exists: the edge matches browser `Origin` values exactly
 * (handled in `sdkwork-web-core`; only private-network/localhost, `app://`,
 * `tauri://` and the WeChat mini-program runtime are special-cased). Every
 * module browser console is served on its own cloud sub-domain and calls the
 * platform API edge cross-origin, so enumerating them costs
 * `labels x 16 base domains x schemes` origins per environment — roughly 3000
 * entries and ~100 KiB for non-production, close enough to the Linux 128 KiB
 * single-environment-variable limit to be a deployment hazard. The pattern
 * expresses the same closure in about 1 KiB.
 *
 * Base domains: APP_RUNTIME_TOPOLOGY_NAMING §9.3 Base Domain Registry.
 * Environment axis: ENVIRONMENT_SPEC §2 (development/test/staging/demo/production).
 *
 * Consumers:
 *   - scripts/derive-cors-allowed-origins.mjs  (materializes every carrier)
 *   - scripts/validate-docker-deployment.mjs   (fail-closed gate)
 *   - scripts/cors-allowed-origins.test.mjs    (unit tests)
 */

import { ENVIRONMENT_HOST_PORTS } from './deployment-environments.mjs';

/**
 * Registered public base-domain family (ENVIRONMENT_SPEC §5.1.0.1). The edge
 * serves `api<env-suffix>.<base-domain>` and every module console
 * `<label><env-suffix>.<base-domain>` for each entry.
 *
 * Mirrors `SDKWORK_REGISTERED_SERVICE_BASE_DOMAINS` in
 * `sdkwork-web-framework/crates/sdkwork-web-core/src/registered_console_hosts.rs`;
 * keep both in step.
 */
export const BASE_DOMAINS = Object.freeze([
  'sdkwork.com',
  'birdcoder.com',
  'dtupay.com',
  'noaper.com',
  'sdkwork.cn',
  'birdcoder.cn',
  'dtupay.cn',
  'noaper.cn',
  'skubc.com',
  'skubc.cn',
  'zowalk.com',
  'zowalk.cn',
  'offer86.com',
  'offer86.cn',
  '86offer.com',
  '86offer.cn',
]);

/** Lifecycle environment -> console host label suffix (production = none). */
export const CONSOLE_ENV_SUFFIX = Object.freeze({
  development: '-dev',
  test: '-test',
  staging: '-staging',
  demo: '-demo',
  production: '',
});

/**
 * Scheme axis. Every non-production console is reachable over plain HTTP on its
 * environment edge port and over HTTPS once TLS terminates in front of it;
 * production consoles are HTTPS-only, so production stays minimal.
 */
export const CONSOLE_ENV_SCHEMES = Object.freeze({
  development: Object.freeze(['http', 'https']),
  test: Object.freeze(['http', 'https']),
  staging: Object.freeze(['http', 'https']),
  demo: Object.freeze(['http', 'https']),
  production: Object.freeze(['https']),
});

/**
 * Registered console host label registry: every `<label>` the platform serves a
 * public host on, without the environment suffix.
 *
 * Derived from the fleet deployment inventory — every host an application
 * declares for itself in `deployments/**\/server.*.toml` (`serverName`),
 * `deployments/**\/nginx*.conf` (`server_name`) or `etc/topology/*.env`
 * (`*_PUBLIC_HTTP_URL`) is reduced to `host - environment suffix - base domain`
 * and kept only when the result is a single DNS label. The reduction is a
 * superset of the previously materialized exact allow-list, so swapping the
 * enumeration for the pattern cannot drop a console.
 *
 * This is *data*, not framework code: covering a new module console means adding
 * its label here, never cutting a `sdkwork-web-framework` release. The `api`
 * label is included on purpose — the API plane host is itself a page origin in
 * some environments (the development PC console is served from
 * `api-dev.<domain>`), and it was present in the previously verified list.
 */
export const WEB_CONSOLE_CLIENT_ROLES = Object.freeze([
  'account',
  'admin',
  'agent',
  'agents',
  'agents-admin',
  'agents-app',
  'agents-open',
  'agentstudio',
  'agentstudio-admin',
  'agentstudio-app',
  'aiot',
  'aiot-admin',
  'aiot-app',
  'api',
  'appbase',
  'appstore',
  'assets',
  'audio',
  'audio-api',
  'birdcoder2',
  'browser',
  'canvas',
  'cloudrouter',
  'cms',
  'code',
  'codebox',
  'community',
  'company',
  'console',
  'course',
  'customer-service',
  'deploy',
  'deploy-admin',
  'deploy-app',
  'dezhou',
  'discovery',
  'discovery-admin',
  'doc',
  'documents',
  'documents-admin',
  'documents-open',
  'doudizhu',
  'drama',
  'drive',
  'drive-internal',
  'events',
  'feeds',
  'forum',
  'gameengine',
  'games',
  'generations',
  'github',
  'github-workflow',
  'harness',
  'iam',
  'im',
  'image',
  'inventory',
  'invoice',
  'kernel',
  'knowledge',
  'knowledgebase',
  'knowledgebase-admin',
  'llm',
  'llm-admin',
  'llm-app',
  'llm-open',
  'local-router',
  'mahjong',
  'mail',
  'mcp',
  'mem',
  'membership',
  'memory',
  'memory-admin',
  'memory-app',
  'memory-open',
  'merchandise',
  'messaging',
  'modelkit',
  'models',
  'music',
  'news',
  'notary',
  'notary-admin',
  'notes',
  'order',
  'partner',
  'payment',
  'portal',
  'promotion',
  'prompts',
  'router',
  'router-admin',
  'router-open',
  'rtc',
  'search',
  'server',
  'server-admin',
  'server-app',
  'settings',
  'shop',
  'skills',
  'terminal',
  'tts',
  'video',
  'video-cut',
  'voice',
  'web-internal',
  'web-provider-events',
  'xiangqi',
]);

/** Environment keys that carry the registered console host pattern. */
export const CONSOLE_HOST_LABELS_ENV_KEY = 'SDKWORK_CORS_CONSOLE_HOST_LABELS';
export const CONSOLE_HOST_SUFFIX_ENV_KEY = 'SDKWORK_CORS_CONSOLE_HOST_SUFFIX';
export const CONSOLE_HOST_SCHEMES_ENV_KEY = 'SDKWORK_CORS_CONSOLE_HOST_SCHEMES';
export const CONSOLE_HOST_BASE_DOMAINS_ENV_KEY = 'SDKWORK_CORS_CONSOLE_HOST_BASE_DOMAINS';

/**
 * Registered client origins the web framework merges into every policy
 * (WEB_FRAMEWORK_SPEC §12), so carriers never need to list them.
 * Mirrors `REGISTERED_SDKWORK_DESKTOP_CORS_ORIGINS` +
 * `REGISTERED_SDKWORK_MINI_PROGRAM_CORS_ORIGINS` in `sdkwork-web-core`.
 */
export const REGISTERED_CLIENT_ORIGINS = Object.freeze([
  'app://dsh',
  'app://birdcoder',
  'app://sdkwork',
  'app://dtupay',
  'tauri://localhost',
  'https://servicewechat.com',
]);

function requireEnvironment(environment) {
  if (!Object.hasOwn(CONSOLE_ENV_SUFFIX, environment)) {
    throw new Error(`unsupported console environment: ${environment}`);
  }
}

/** `router`, `router-admin`, `server-app`, ... -> canonical console host. */
export function consoleHost(role, environment) {
  requireEnvironment(environment);
  return `${role}${CONSOLE_ENV_SUFFIX[environment]}`;
}

/** Every console host label registered for the platform (sorted, deduped). */
export function consoleHostLabelRegistry() {
  return [...new Set(WEB_CONSOLE_CLIENT_ROLES)].sort();
}

/** Registry as the comma-separated value of the labels environment key. */
export function consoleHostLabelsCsv() {
  return consoleHostLabelRegistry().join(',');
}

/** Environment suffix carried by the suffix environment key. */
export function consoleHostSuffix(environment) {
  requireEnvironment(environment);
  return CONSOLE_ENV_SUFFIX[environment];
}

/** Accepted schemes for `environment` (sorted, deduped). */
export function consoleHostSchemes(environment) {
  requireEnvironment(environment);
  return [...new Set(CONSOLE_ENV_SCHEMES[environment])].sort();
}

/** Schemes as the comma-separated value of the schemes environment key. */
export function consoleHostSchemesCsv(environment) {
  return consoleHostSchemes(environment).join(',');
}

/** The complete registered console host configuration for `environment`. */
export function canonicalConsoleHostConfig(environment) {
  return Object.freeze({
    labels: consoleHostLabelsCsv(),
    suffix: consoleHostSuffix(environment),
    schemes: consoleHostSchemesCsv(environment),
  });
}

/**
 * Console host labels for one environment, longest first so that
 * `router-admin-dev` wins over `router-dev` when stripping prefixes.
 */
export function consoleHostLabels(environment) {
  requireEnvironment(environment);
  return consoleHostLabelRegistry()
    .map((role) => `${role}${CONSOLE_ENV_SUFFIX[environment]}`)
    .sort((a, b) => b.length - a.length);
}

/**
 * Every canonical console origin for one environment (sorted, deduped).
 *
 * Retained for gates and tests that need the concrete closure; deployments carry
 * the pattern instead of this list.
 */
export function canonicalConsoleOrigins(environment) {
  requireEnvironment(environment);
  const origins = new Set();
  for (const role of consoleHostLabelRegistry()) {
    for (const domain of BASE_DOMAINS) {
      for (const scheme of CONSOLE_ENV_SCHEMES[environment]) {
        origins.add(`${scheme}://${role}${CONSOLE_ENV_SUFFIX[environment]}.${domain}`);
      }
    }
  }
  return [...origins].sort();
}

/**
 * Non-console origins every carrier keeps verbatim: the per-environment loopback
 * surface operators use to reach the edge locally. The framework only widens a
 * loopback origin when the allow-list carries the `http://localhost` sentinel, so
 * the concrete per-port entries are load-bearing. Desktop shell and
 * mini-program origins are deliberately absent — the web framework merges them
 * into every policy, so repeating them in a carrier only invites drift.
 */
export function canonicalSeedOrigins(environment) {
  requireEnvironment(environment);
  const port = ENVIRONMENT_HOST_PORTS[environment];
  return [`http://localhost:${port}`, `http://127.0.0.1:${port}`];
}

/** True when `origin` is one of the registered desktop/mini-program origins. */
export function isRegisteredClientOrigin(origin) {
  return REGISTERED_CLIENT_ORIGINS.includes(origin);
}

/**
 * True when `origin` is a console origin owned by the canonical registry for any
 * lifecycle environment, i.e. a console entry the pattern already covers and
 * that must not be repeated in the exact allow-list.
 *
 * The host must match `<label><suffix>.<registered base domain>` exactly. Prefix
 * matching would swallow unrelated hosts such as `router.open.sdkwork.com`.
 */
export function isManagedConsoleOrigin(origin) {
  const match = /^https?:\/\/([^/]+)$/u.exec(origin);
  if (!match) return false;
  const host = match[1];
  for (const environment of Object.keys(CONSOLE_ENV_SUFFIX)) {
    for (const label of consoleHostLabels(environment)) {
      for (const domain of BASE_DOMAINS) {
        if (host === `${label}.${domain}`) return true;
      }
    }
  }
  return false;
}

/**
 * True when `origin` is exactly one of the canonical console origins for
 * `environment` (host must match and the scheme must be allowed).
 */
export function isCanonicalConsoleOrigin(origin, environment) {
  requireEnvironment(environment);
  const match = /^(https?):\/\/([^/]+)$/u.exec(origin);
  if (!match) return false;
  const [, scheme, host] = match;
  if (!CONSOLE_ENV_SCHEMES[environment].includes(scheme)) return false;
  for (const label of consoleHostLabels(environment)) {
    for (const domain of BASE_DOMAINS) {
      if (host === `${label}.${domain}`) return true;
    }
  }
  return false;
}

/**
 * True when `origin` is covered by the registered console host pattern for
 * `environment`, i.e. the deployment's pattern would allow it.
 */
export function isPatternCoveredOrigin(origin, environment) {
  requireEnvironment(environment);
  const match = /^(https?):\/\/([^/]+)$/u.exec(origin);
  if (!match) return false;
  const [, scheme, hostPort] = match;
  if (!CONSOLE_ENV_SCHEMES[environment].includes(scheme)) return false;
  const host = hostPort.replace(/:\d+$/, '');
  return consoleHostLabels(environment).some((label) =>
    BASE_DOMAINS.some((domain) => host === `${label}.${domain}`),
  );
}

/**
 * True when `host` is a console host for `environment`, i.e. the deployment's
 * registered console host pattern accepts it.
 */
export function isConsoleHostForEnvironment(host, environment) {
  requireEnvironment(environment);
  return consoleHostLabels(environment).some((label) =>
    BASE_DOMAINS.some((domain) => host === `${label}.${domain}`),
  );
}

/**
 * Protocol-agnostic host of an exact http(s) origin, or undefined when the
 * origin is not a plain browser origin (app://, tauri://, ...).
 */
export function httpHostOf(origin) {
  const match = /^https?:\/\/([^/]+)$/u.exec(origin);
  return match ? match[1] : undefined;
}

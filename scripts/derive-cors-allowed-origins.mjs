#!/usr/bin/env node
/**
 * SDKWork platform API gateway CORS carrier materializer (single source).
 *
 * WHY: the gateway matches browser `Origin` exactly, and every module console is
 * served on its own cloud sub-domain and calls the edge cross-origin. The
 * canonical allow-list is therefore the closure declared in
 * `scripts/cors-allowed-origins.mjs`:
 *
 *   console label  x  registered base domain  x  lifecycle environment  x  scheme
 *
 * Enumerating that closure costs roughly 3000 exact origins (~100 KiB) per
 * non-production environment — uncomfortably close to the Linux 128 KiB single
 * environment-variable limit (`MAX_ARG_STRLEN`). Carriers therefore declare the
 * *pattern* (`SDKWORK_CORS_CONSOLE_HOST_{LABELS,SUFFIX,SCHEMES}`, matched
 * exactly per component by `sdkwork-web-core`) plus the exact origins the
 * pattern cannot express (per-environment loopback seeds). Desktop-shell and
 * mini-program origins are merged by the framework and are never listed.
 *
 * The pattern matches one registered label per host, so operator-chosen brand
 * hosts (`platform.birdcoder.com`), the apex (`sdkwork.com`) and nested names
 * (`region.console.sdkwork.com`) are declared through the opt-in whole
 * sub-domain rule (`SDKWORK_CORS_SUBDOMAIN_{BASE_DOMAINS,SCHEMES}`,
 * `CORS_SPEC.md` §5.1). It is bounded by the registered base-domain registry.
 *
 * Any carrier left behind silently blocks the consoles it omits — the historical
 * failure mode was "No 'Access-Control-Allow-Origin' header is present on the
 * requested resource" on the OPTIONS preflight of
 * `http://api-dev.sdkwork.com/app/v3/api/oauth/scan_login_modes` from
 * `http://im-dev.sdkwork.com`, because the webserver install bundle's gateway
 * sibling compose had frozen a 12-entry development-only literal.
 *
 * Carrier families (39 carriers):
 *   1. etc/sdkwork-api-cloud-gateway.<env>.toml            [cors] + [cors.registeredConsoleHosts]
 *   2. docker/config/config.toml                           same TOML shape
 *   3. docker/env/<env>.env (+ *.env.example), docker/.env.example
 *                                                          GATEWAY_CORS_*
 *   4. docker-compose.yml                                  ${GATEWAY_CORS_*:-<dev default>}
 *   5. docker-compose.bundle.yml                           ${GATEWAY_CORS_*:-}
 *   6. ../sdkwork-webserver/deployments/docker/env/<env>.env (+ *.env.example)
 *        SDKWORK_MODULE_API_GATEWAY_CORS_*  the bundled gateway sibling
 *        SDKWORK_CORS_CONSOLE_HOST_*        the webserver edge's own policy
 *   7. ../sdkwork-webserver/deployments/docker/docker-compose.bundle-gateway.yml
 *                                                          ${SDKWORK_MODULE_API_GATEWAY_CORS_*:-}
 *
 * Usage:
 *   node scripts/derive-cors-allowed-origins.mjs                 # dry run (default)
 *   node scripts/derive-cors-allowed-origins.mjs --apply         # rewrite carriers
 *   node scripts/derive-cors-allowed-origins.mjs --check         # fail on drift (gate)
 *   node scripts/derive-cors-allowed-origins.mjs --check --json  # machine readable
 *   node scripts/derive-cors-allowed-origins.mjs --no-sibling    # skip sdkwork-webserver
 *
 * `--check` requires the sibling app for families 6/7; pass `--no-sibling` when
 * only the gateway repository is checked out.
 */

import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { ENVIRONMENTS } from './deployment-environments.mjs';
import {
  canonicalConsoleHostConfig,
  canonicalSeedOrigins,
  canonicalSubdomainHostConfig,
  consoleHostLabelRegistry,
} from './cors-allowed-origins.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(__dirname, '..');
const SIBLING_ROOT = join(REPO_ROOT, '..', 'sdkwork-webserver');

const APPLY = process.argv.includes('--apply');
const CHECK = process.argv.includes('--check');
const JSON_OUT = process.argv.includes('--json');
const NO_SIBLING = process.argv.includes('--no-sibling');

if (APPLY && CHECK) {
  console.error('--apply and --check are mutually exclusive');
  process.exit(2);
}

// ---------------------------------------------------------------------------
// Canonical expected values.
// ---------------------------------------------------------------------------
/**
 * Environment-variable prefixes. The gateway reads the shared `SDKWORK_CORS_*`
 * keys, but its compose interpolates the host-side `GATEWAY_CORS_*` values, and
 * the webserver's bundled gateway sibling consumes the module-scoped key so the
 * two gateway families can never disagree.
 */
const GATEWAY_ENV_PREFIX = 'GATEWAY_';
const SIBLING_ENV_PREFIX = 'SDKWORK_MODULE_API_GATEWAY_';
const WEBSERVER_ENV_PREFIX = 'SDKWORK_';

/** The variables every carrier family materializes, per prefix. */
function keysFor(prefix) {
  return {
    allowedOrigins: `${prefix}CORS_ALLOWED_ORIGINS`,
    labels: `${prefix}CORS_CONSOLE_HOST_LABELS`,
    suffix: `${prefix}CORS_CONSOLE_HOST_SUFFIX`,
    schemes: `${prefix}CORS_CONSOLE_HOST_SCHEMES`,
    subdomainBaseDomains: `${prefix}CORS_SUBDOMAIN_BASE_DOMAINS`,
    subdomainSchemes: `${prefix}CORS_SUBDOMAIN_SCHEMES`,
  };
}

/** The compose mapping names (gateway container contract). */
const COMPOSE_MAP_KEYS = {
  allowedOrigins: 'SDKWORK_CORS_ALLOWED_ORIGINS',
  labels: 'SDKWORK_CORS_CONSOLE_HOST_LABELS',
  suffix: 'SDKWORK_CORS_CONSOLE_HOST_SUFFIX',
  schemes: 'SDKWORK_CORS_CONSOLE_HOST_SCHEMES',
  subdomainBaseDomains: 'SDKWORK_CORS_SUBDOMAIN_BASE_DOMAINS',
  subdomainSchemes: 'SDKWORK_CORS_SUBDOMAIN_SCHEMES',
};

/** The registered sub-domain rule every gateway carrier declares. */
function canonicalSubdomainConfig() {
  return canonicalSubdomainHostConfig();
}

/**
 * The values every carrier declares for `environment`, keyed by variable name.
 *
 * `allowedOrigins` is the loopback seed list only: the console closure is
 * expressed by the pattern, and frameworks merge the registered client origins.
 * `subdomainBaseDomains`/`subdomainSchemes` are the opt-in whole-sub-domain rule
 * (`CORS_SPEC.md` §5.1).
 */
function expectedValues(
  environment,
  prefix,
  { includeAllowedOrigins = true, includeSubdomains = true } = {},
) {
  const keys = keysFor(prefix);
  const config = canonicalConsoleHostConfig(environment);
  const expected = new Map();
  if (includeAllowedOrigins) {
    expected.set(keys.allowedOrigins, canonicalSeedOrigins(environment).join(','));
  }
  expected.set(keys.labels, config.labels);
  expected.set(keys.suffix, config.suffix);
  expected.set(keys.schemes, config.schemes);
  if (includeSubdomains) {
    const subdomain = canonicalSubdomainConfig();
    expected.set(keys.subdomainBaseDomains, subdomain.baseDomains);
    expected.set(keys.subdomainSchemes, subdomain.schemes);
  }
  return expected;
}

// ---------------------------------------------------------------------------
// Carrier 1/2: TOML profiles.
// ---------------------------------------------------------------------------
const CONSOLE_HOSTS_HEADER = '[cors.registeredConsoleHosts]';
const SUBDOMAIN_HOSTS_HEADER = '[cors.registeredSubdomainHosts]';

function tomlArray(values) {
  return `[${values.map((value) => `"${value}"`).join(', ')}]`;
}

function tomlCarrier(file, environment) {
  const render = (expected) => {
    const keys = keysFor(GATEWAY_ENV_PREFIX);
    return {
      originLine: `allowedOrigins = ${tomlArray(expected.get(keys.allowedOrigins).split(','))}`,
      block: [
        CONSOLE_HOSTS_HEADER,
        `labels = ${tomlArray(expected.get(keys.labels).split(','))}`,
        `environmentSuffix = "${expected.get(keys.suffix)}"`,
        `schemes = ${tomlArray(expected.get(keys.schemes).split(','))}`,
        '',
        SUBDOMAIN_HOSTS_HEADER,
        `baseDomains = ${tomlArray(expected.get(keys.subdomainBaseDomains).split(','))}`,
        `schemes = ${tomlArray(expected.get(keys.subdomainSchemes).split(','))}`,
      ].join('\n'),
    };
  };

  return {
    id: `toml:${file}`,
    file: join(REPO_ROOT, file),
    environment,
    expected: () => expectedValues(environment, GATEWAY_ENV_PREFIX),
    isCanonical(text) {
      const { originLine, block } = render(this.expected());
      const detail = [];
      if (!text.includes(originLine)) detail.push('allowedOrigins');
      if (!text.includes(block)) detail.push(`${CONSOLE_HOSTS_HEADER} + ${SUBDOMAIN_HOSTS_HEADER}`);
      return { canonical: detail.length === 0, detail };
    },
    apply(text) {
      const { originLine, block } = render(this.expected());
      let next = text.replace(/allowedOrigins\s*=\s*\[[^\]]*\]/u, originLine);
      const start = next.indexOf(CONSOLE_HOSTS_HEADER);
      if (start < 0) {
        // Keep both halves of the policy adjacent: the sub-tables follow the
        // `[cors]` origins line they refine.
        return next.replace(originLine, `${originLine}\n\n${block}`);
      }
      // The expected block spans both sub-tables, so the replacement ends at the
      // first table after the sub-domain section (or at end of file).
      const subdomainStart = next.indexOf(SUBDOMAIN_HOSTS_HEADER, start);
      const end = subdomainStart < 0
        ? next.indexOf('\n[', start + CONSOLE_HOSTS_HEADER.length)
        : next.indexOf('\n[', subdomainStart + SUBDOMAIN_HOSTS_HEADER.length);
      const tail = end < 0 ? '\n' : next.slice(end + 1);
      return `${next.slice(0, start)}${block}\n\n${tail}`;
    },
  };
}

// ---------------------------------------------------------------------------
// Carrier 3/6: env files.
// ---------------------------------------------------------------------------
function envCarrier(file, environment, groups, options = {}) {
  return {
    id: `env:${file}`,
    file: options.sibling ? join(SIBLING_ROOT, file) : join(REPO_ROOT, file),
    environment,
    optional: options.optional === true,
    // One carrier per file: several families share an env file (the webserver
    // deployment env carries both its bundled gateway's keys and its own), and
    // two carriers writing the same file would race on stale text.
    expected: () => {
      const merged = new Map();
      for (const group of groups) {
        for (const [key, value] of expectedValues(environment, group.prefix, group)) {
          merged.set(key, value);
        }
      }
      return merged;
    },
    isCanonical(text) {
      const detail = [];
      for (const [key, wanted] of this.expected()) {
        const matches = [...text.matchAll(new RegExp(`^${key}=([^\\r\\n]*)$`, 'gmu'))];
        if (matches.length === 0) {
          detail.push(`${key} (missing)`);
          continue;
        }
        // A duplicated key is ambiguous: compose and the runtime config loader
        // disagree about which occurrence wins, so flag it as drift.
        if (matches.length > 1) detail.push(`${key} (declared ${matches.length}x)`);
        if (matches[0][1].trim() !== wanted) {
          detail.push(`${key} (${matches[0][1].trim().length} B != ${wanted.length} B)`);
        }
      }
      return { canonical: detail.length === 0, detail };
    },
    apply(text) {
      let next = text;
      for (const [key, wanted] of this.expected()) {
        const pattern = new RegExp(`^${key}=([^\\r\\n]*)$`, 'gmu');
        const matches = [...next.matchAll(pattern)];
        // Collapse duplicates down to the first occurrence, then normalize it.
        for (let index = matches.length - 1; index >= 1; index -= 1) {
          const match = matches[index];
          let end = match.index + match[0].length;
          if (next.startsWith('\r\n', end)) end += 2;
          else if (next[end] === '\n') end += 1;
          next = `${next.slice(0, match.index)}${next.slice(end)}`;
        }
        const line = `${key}=${wanted}`;
        if (matches.length > 0) {
          next = next.replace(new RegExp(`^${key}=[^\\r\\n]*$`, 'mu'), line);
          continue;
        }
        // Preserve the file's line-ending convention (CRLF drift has already
        // broken container startup gates in this workspace).
        const eol = next.includes('\r\n') ? '\r\n' : '\n';
        // Keep the CORS policy block together: a new key lands next to the last
        // existing CORS key instead of drifting to the end of the file.
        const anchor = [
          ...next.matchAll(/^(?:GATEWAY|SDKWORK)_[A-Z0-9_]*CORS[A-Z0-9_]*=[^\r\n]*$/gmu),
        ].pop();
        if (anchor) {
          const at = anchor.index + anchor[0].length;
          next = `${next.slice(0, at)}${eol}${line}${next.slice(at)}`;
          continue;
        }
        const separator = next.length === 0 || /\r?\n$/u.test(next) ? '' : eol;
        next = `${next}${separator}${line}${eol}`;
      }
      return next;
    },
  };
}

// ---------------------------------------------------------------------------
// Carrier 4/5/7: compose files.
// ---------------------------------------------------------------------------
/**
 * Locate a `KEY: value` mapping entry without consuming the line terminator that
 * precedes it. Anchoring with `^\s*` is unsafe: JavaScript treats `\r` as a line
 * terminator, so `^` also matches between the `\r` and `\n` of a CRLF pair and a
 * greedy `\s*` then swallows the `\n`, silently rewriting the file to mixed line
 * endings (CRLF drift has already broken container startup gates here).
 */
function locateComposeEntry(text, key) {
  const match = new RegExp(`(^|[\\r\\n]+)([ \\t]*)${key}:([^\\r\\n]*)`).exec(text);
  if (!match) return undefined;
  return {
    indent: match[2],
    value: match[3].trim(),
    start: match.index + match[1].length,
    end: match.index + match[0].length,
  };
}

/**
 * End offset of the contiguous CORS mapping block that begins at `start`.
 *
 * The block is a run of adjacent `SDKWORK_*CORS*` compose mapping lines; the
 * console-host and sub-domain keys live in the same block, so a rewrite replaces
 * the whole run instead of appending a second copy beside the old lines.
 */
function composeCorsBlockEnd(text, start) {
  let index = start;
  let end = start;
  while (index < text.length) {
    const lineEnd = text.indexOf('\n', index);
    const lineStop = lineEnd < 0 ? text.length : lineEnd;
    const line = text.slice(index, lineStop).replace(/\r$/u, '');
    if (!/^[ \t]*(?:SDKWORK|GATEWAY)_[A-Z0-9_]*CORS[A-Z0-9_]*\s*:/u.test(line)) break;
    end = lineStop;
    if (lineEnd < 0) break;
    index = lineEnd + 1;
  }
  return end;
}

function composeCarrier(
  file,
  environment,
  prefix,
  { emptyDefaults = true, sibling = false, includeSubdomains = true } = {},
) {
  const keys = keysFor(prefix);
  const mapKeys = Object.fromEntries(
    Object.entries(COMPOSE_MAP_KEYS).filter(
      ([field]) => includeSubdomains || !field.startsWith('subdomain'),
    ),
  );
  const defaults = () => {
    if (emptyDefaults) {
      return {
        allowedOrigins: '',
        labels: '',
        suffix: '',
        schemes: '',
        subdomainBaseDomains: '',
        subdomainSchemes: '',
      };
    }
    const config = canonicalConsoleHostConfig(environment);
    const subdomain = canonicalSubdomainConfig();
    return {
      allowedOrigins: canonicalSeedOrigins(environment).join(','),
      labels: config.labels,
      suffix: config.suffix,
      schemes: config.schemes,
      subdomainBaseDomains: subdomain.baseDomains,
      subdomainSchemes: subdomain.schemes,
    };
  };
  return {
    id: `compose:${file}`,
    file: sibling ? join(SIBLING_ROOT, file) : join(REPO_ROOT, file),
    environment,
    expected: () => keys,
    isCanonical(text) {
      const detail = [];
      for (const field of Object.keys(mapKeys)) {
        const occurrences = [
          ...text.matchAll(new RegExp(`^[ \\t]*${mapKeys[field]}\\s*:`, 'gmu')),
        ];
        if (occurrences.length === 0) {
          detail.push(`${mapKeys[field]} (missing)`);
          continue;
        }
        if (occurrences.length > 1) {
          detail.push(`${mapKeys[field]} (declared ${occurrences.length}x)`);
          continue;
        }
        const entry = locateComposeEntry(text, mapKeys[field]);
        const value = entry.value.replace(/^'|'$/gu, '');
        if (!value.startsWith(`\${${keys[field]}:-`)) detail.push(mapKeys[field]);
      }
      return { canonical: detail.length === 0, detail };
    },
    apply(text) {
      const entry = locateComposeEntry(text, mapKeys.allowedOrigins);
      if (!entry) return text;
      const fallback = defaults();
      const lines = Object.keys(mapKeys)
        .map((field) => `${entry.indent}${mapKeys[field]}: \${${keys[field]}:-${fallback[field]}}`)
        .join('\n');
      const end = composeCorsBlockEnd(text, entry.start);
      return `${text.slice(0, entry.start)}${lines}${text.slice(end)}`;
    },
  };
}

// ---------------------------------------------------------------------------
// Carrier inventory.
// ---------------------------------------------------------------------------
const carriers = [];

// Families 1-2: source-config and container runtime TOML profiles.
for (const environment of ENVIRONMENTS) {
  if (environment === 'demo') continue; // demo reuses the development profile.
  if (!existsSync(join(REPO_ROOT, `etc/sdkwork-api-cloud-gateway.${environment}.toml`))) continue;
  carriers.push(tomlCarrier(`etc/sdkwork-api-cloud-gateway.${environment}.toml`, environment));
}
carriers.push(tomlCarrier('docker/config/config.toml', 'production'));

// Family 3: gateway env files and their committed templates. The concrete
// `<env>.env` files are gitignored runtime carriers, so a clean checkout has
// only the committed `.env.example` templates; validate the runtime file when
// it exists (deploy hosts) and never fail on its absence.
for (const environment of ENVIRONMENTS) {
  const groups = [{ prefix: GATEWAY_ENV_PREFIX }];
  carriers.push(envCarrier(`docker/env/${environment}.env`, environment, groups, { optional: true }));
  carriers.push(envCarrier(`docker/env/${environment}.env.example`, environment, groups));
}
carriers.push(
  envCarrier('docker/.env.example', 'development', [{ prefix: GATEWAY_ENV_PREFIX }]),
);

// Families 4-5: single-node default and the install-bundle composition.
carriers.push(
  composeCarrier('docker-compose.yml', 'development', GATEWAY_ENV_PREFIX, { emptyDefaults: false }),
);
carriers.push(composeCarrier('docker-compose.bundle.yml', 'production', GATEWAY_ENV_PREFIX));

// Families 6-7: the webserver install bundle's gateway sibling and the edge's
// own policy. The sibling carries the SAME per-environment pattern as the
// gateway bundle env file so the two gateway families can never disagree.
if (!NO_SIBLING && existsSync(SIBLING_ROOT)) {
  for (const environment of ENVIRONMENTS) {
    if (!existsSync(join(SIBLING_ROOT, `deployments/docker/env/${environment}.env.example`))) {
      continue;
    }
    for (const file of [
      `deployments/docker/env/${environment}.env`,
      `deployments/docker/env/${environment}.env.example`,
    ]) {
      // The same env file feeds the bundled gateway sibling
      // (`SDKWORK_MODULE_API_GATEWAY_CORS_*`) and the webserver edge's own
      // policy (`SDKWORK_CORS_CONSOLE_HOST_*`); both are managed by one carrier
      // so the two gateway families can never disagree. The edge's exact
      // allow-list stays governed by its own repository contract.
      carriers.push(
        envCarrier(
          file,
          environment,
          [
            { prefix: SIBLING_ENV_PREFIX, includeSubdomains: false },
            { prefix: WEBSERVER_ENV_PREFIX, includeAllowedOrigins: false, includeSubdomains: false },
          ],
          { sibling: true, optional: file.endsWith('.env') },
        ),
      );
    }
  }
  carriers.push(
    composeCarrier(
      'deployments/docker/docker-compose.bundle-gateway.yml',
      'production',
      SIBLING_ENV_PREFIX,
      { sibling: true, includeSubdomains: false },
    ),
  );
}

// ---------------------------------------------------------------------------
// Run.
// ---------------------------------------------------------------------------
function rel(file) {
  return file.startsWith(SIBLING_ROOT)
    ? `../sdkwork-webserver/${file.slice(SIBLING_ROOT.length + 1)}`
    : file.slice(REPO_ROOT.length + 1);
}

const results = [];
for (const carrier of carriers) {
  if (!existsSync(carrier.file)) {
    if (carrier.optional) continue;
    results.push({ carrier, relative: rel(carrier.file), error: 'carrier file is missing' });
    continue;
  }
  const text = readFileSync(carrier.file, 'utf8');
  const { canonical, detail } = carrier.isCanonical(text);
  results.push({
    carrier,
    relative: rel(carrier.file),
    environment: carrier.environment,
    canonical,
    detail,
    text,
  });
}

const drifted = results.filter((result) => result.error || !result.canonical);

if (!JSON_OUT) {
  for (const result of results) {
    if (result.error) {
      console.log(`FAIL ${result.relative}\n     ${result.error}`);
      continue;
    }
    const status = result.canonical ? 'OK  ' : 'FAIL';
    const detail = result.canonical ? '' : `\n     ${result.detail.slice(0, 4).join('; ')}`;
    console.log(`${status} ${result.carrier.environment.padEnd(11)} ${result.relative}${detail}`);
  }
}

if (CHECK) {
  const summary = {
    ok: drifted.length === 0,
    carriers: results.length,
    labels: consoleHostLabelRegistry().length,
    drifted: drifted.map((result) => ({
      file: result.relative,
      environment: result.carrier?.environment,
      error: result.error,
      detail: result.detail,
    })),
  };
  if (JSON_OUT) console.log(JSON.stringify(summary, null, 2));
  else if (summary.ok) console.log(`\nAll ${results.length} CORS carriers are canonical.`);
  else {
    console.error(
      `\n${drifted.length} CORS carrier(s) drifted from the canonical policy.` +
        '\nRun: node scripts/derive-cors-allowed-origins.mjs --apply',
    );
  }
  process.exit(summary.ok ? 0 : 1);
}

if (!APPLY) {
  console.log(
    `\nDry run. carriers=${results.length} drifted=${drifted.length} ` +
      `(labels=${consoleHostLabelRegistry().length}). ` +
      'Re-run with --apply to rewrite the CORS carriers.',
  );
  process.exit(0);
}

let applied = 0;
for (const result of drifted) {
  if (result.error) continue;
  writeFileSync(result.carrier.file, result.carrier.apply(result.text), 'utf8');
  applied += 1;
  console.log(`applied ${result.relative} [${result.carrier.id.split(':')[0]}]`);
}
console.log(`Done. ${applied} carrier(s) rewritten, ${results.length - applied} already canonical.`);

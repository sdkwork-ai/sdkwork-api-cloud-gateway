#!/usr/bin/env node
// SDKWork platform API gateway CORS client-origin derivation (single source).
//
// WHY: the gateway's allowed browser Origins were previously only the api-*
// edge self-origins, desktop WebView shells (app://, tauri://) and the WeChat
// mini-program runtime. Module browser consoles served on their own public
// cloud subdomains (e.g. router-dev.sdkwork.com) call the platform api edge
// cross-origin and were blocked by the gateway's exact-match CORS. This tool
// expands every registered platform-API web-console client role across the
// registered base-domain family per lifecycle environment, so browser Origins
// are allowed regardless of which product base domain a console is served on.
//
// It materializes the SAME canonical origin set into every file that carries
// the gateway CORS allow-list, so the TOML [cors].allowedOrigins and the
// docker-compose runtime env (GATEWAY_CORS_ALLOWED_ORIGINS /
// SDKWORK_CORS_ALLOWED_ORIGINS) never drift:
//   - etc/sdkwork-api-cloud-gateway.{development,test,staging,production}.toml
//   - docker/config/config.toml
//   - docker/env/{development,test,staging,production,demo}.env
//   - docker/.env.example and docker/env/*.env.example
//   - (external) webserver bundle-gateway compose SDKWORK_CORS_ALLOWED_ORIGINS
//
// Existing non-console seeds (api-*, localhost, desktop, mini-program) are
// preserved; new module-console origins are appended (sorted, deduped).
//
// Usage:
//   node scripts/derive-cors-allowed-origins.mjs            # print JSON draft
//   node scripts/derive-cors-allowed-origins.mjs --apply     # rewrite files
//
// Add a module browser console to WEB_CONSOLE_CLIENT_ROLES when it becomes a
// cross-origin web client of the platform API edge.

import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(__dirname, '..');

const APPLY = process.argv.includes('--apply');
const ALSO_BUNDLE_GATEWAY = process.argv.includes('--bundle');

// ---------------------------------------------------------------------------
// Registered product base domains (APP_RUNTIME_TOPOLOGY_NAMING.md §9.3).
// ---------------------------------------------------------------------------
const BASE_DOMAINS = [
  'sdkwork.com', 'birdcoder.com', 'dtupay.com', 'noaper.com',
  'sdkwork.cn', 'birdcoder.cn', 'dtupay.cn', 'noaper.cn',
  'skubc.com', 'skubc.cn', 'zowalk.com', 'zowalk.cn',
  'offer86.com', 'offer86.cn', '86offer.com', '86offer.cn',
];

// Lifecycle environment -> role-label suffix (production = bare role).
const ENV_SUFFIX = {
  development: '-dev',
  test: '-test',
  staging: '-staging',
  production: '',
  demo: '-demo',
};

// Canonical web-console client roles that call the platform API edge from a
// browser on their own cloud subdomain.
const WEB_CONSOLE_CLIENT_ROLES = [
  'router',      // sdkwork-cloudrouter
  'appstore',    // sdkwork-appstore
  'drive',       // sdkwork-drive
  'iam',         // sdkwork-iam
  'im',          // sdkwork-im
  'admin',       // sdkwork-manager
  'code',        // sdkwork-birdcoder
  'portal',      // sdkwork-portal
  'account',     // sdkwork-account
  'agents',      // sdkwork-agents
  'agentstudio', // sdkwork-agentstudio
  'settings',    // sdkwork-settings
  'cms',         // sdkwork-cms
  'assets',      // sdkwork-assets
  'community',   // sdkwork-community
];

function consoleOriginsForRole(role, environment, schemes) {
  const suffix = ENV_SUFFIX[environment];
  const out = [];
  for (const baseDomain of BASE_DOMAINS) {
    const host = `${role}${suffix}.${baseDomain}`;
    for (const scheme of schemes) out.push(`${scheme}://${host}`);
  }
  return out;
}

// ---------------------------------------------------------------------------
// TOML form:  allowedOrigins = ["a", "b", ...]
// ---------------------------------------------------------------------------
function tomlOfFile(file) {
  return {
    file,
    environment: {
      'etc/sdkwork-api-cloud-gateway.development.toml': 'development',
      'etc/sdkwork-api-cloud-gateway.test.toml': 'test',
      'etc/sdkwork-api-cloud-gateway.staging.toml': 'staging',
      'etc/sdkwork-api-cloud-gateway.production.toml': 'production',
      'docker/config/config.toml': 'production',
    }[file],
    schemesOf(env) {
      return env === 'production' ? ['https'] : ['https', 'http'];
    },
    parse(text) {
      const m = text.match(/allowedOrigins\s*=\s*(\[[^\]]*\])/);
      if (!m) return null;
      return m[1].slice(1, -1).split(',').map((s) => s.trim().replace(/^"|"$/g, '')).filter(Boolean);
    },
    render(origins) {
      return `allowedOrigins = [${origins.map((o) => `"${o}"`).join(', ')}]`;
    },
    replace(text, line) {
      return text.replace(/allowedOrigins\s*=\s*\[[^\]]*\]/, line);
    },
  };
}

// ---------------------------------------------------------------------------
// Docker env form:  GATEWAY_CORS_ALLOWED_ORIGINS=a,b,c
// Existing api/localhost/desktop seeds reveal the file's single scheme.
// ---------------------------------------------------------------------------
function envFileOf(file, environment) {
  return {
    file,
    environment,
    schemesOf(env, text) {
      const m = text.match(/GATEWAY_CORS_ALLOWED_ORIGINS=(https?):\/\//);
      return [m ? m[1] : 'http'];
    },
    parse(text) {
      const m = text.match(/^GATEWAY_CORS_ALLOWED_ORIGINS=(.*)$/m);
      if (!m) return null;
      return m[1].split(',').map((s) => s.trim()).filter(Boolean);
    },
    render(origins) {
      return `GATEWAY_CORS_ALLOWED_ORIGINS=${origins.join(',')}`;
    },
    replace(text, line) {
      return text.replace(/^GATEWAY_CORS_ALLOWED_ORIGINS=.*$/m, line);
    },
  };
}

// ---------------------------------------------------------------------------
// Compose form:  SDKWORK_CORS_ALLOWED_ORIGINS: 'a,b,c'
// ---------------------------------------------------------------------------
function composeOfFile(file, environment) {
  return {
    file,
    environment,
    schemesOf() {
      return ['http'];
    },
    parse(text) {
      const m = text.match(/SDKWORK_CORS_ALLOWED_ORIGINS:\s*'([^']*)'/);
      if (!m) return null;
      return m[1].split(',').map((s) => s.trim()).filter(Boolean);
    },
    render(origins) {
      return `      SDKWORK_CORS_ALLOWED_ORIGINS: '${origins.join(',')}'`;
    },
    replace(text, line) {
      return text.replace(/      SDKWORK_CORS_ALLOWED_ORIGINS:\s*'[^']*'/, line);
    },
  };
}

// ---------------------------------------------------------------------------
// Generic merge + write.
// ---------------------------------------------------------------------------
// A seed is kept only when it is a concrete exact origin (no `{`,`}`,`*`
// placeholders, no path). Broken/placeholder entries that were committed
// earlier (e.g. `http://router-admin-{s}.sdkwork.com`) must not be preserved.
function isConcreteOrigin(origin) {
  if (!origin || /[{}*]/.test(origin)) return false;
  if (/^(app|tauri):\/\//.test(origin)) return true;
  const m = origin.match(/^https?:\/\/([^/]+)$/);
  if (!m) return false;
  const host = m[1];
  if (host.includes('@') || host.includes('/')) return false;
  return /^[a-z0-9.\-]+(?::\d+)?$/i.test(host);
}

function mergeOrigins(existing, extra) {
  const out = [];
  const seen = new Set();
  const push = (o) => {
    if (!o || seen.has(o)) return;
    if (!isConcreteOrigin(o)) return; // drop placeholders/invalid seeds
    seen.add(o);
    out.push(o);
  };
  for (const o of existing) push(o);
  for (const o of extra) push(o);
  return out;
}

function processTarget(target) {
  const path = join(REPO_ROOT, target.file);
  if (!existsSync(path)) return null;
  const text = readFileSync(path, 'utf8');
  const existing = target.parse(text);
  if (!existing) return null;
  const schemes = target.schemesOf(target.environment, text);
  const extra = [];
  for (const role of WEB_CONSOLE_CLIENT_ROLES) {
    extra.push(...consoleOriginsForRole(role, target.environment, schemes));
  }
  return { file: target.file, environment: target.environment, seedCount: existing.length, origins: mergeOrigins(existing, extra) };
}

const targets = [
  // Config TOML sources (array form).
  ...['etc/sdkwork-api-cloud-gateway.development.toml', 'etc/sdkwork-api-cloud-gateway.test.toml',
     'etc/sdkwork-api-cloud-gateway.staging.toml', 'etc/sdkwork-api-cloud-gateway.production.toml',
     'docker/config/config.toml'].map((f) => tomlOfFile(f)),
  // Docker runtime env (CSV form) — the compose runtime injection source.
  ...Object.entries({ development: 'docker/env/development.env', test: 'docker/env/test.env',
    staging: 'docker/env/staging.env', production: 'docker/env/production.env',
    demo: 'docker/env/demo.env' }).map(([env, f]) => envFileOf(f, env)),
  // Templates (.env.example + docker/.env.example).
  ...[
    ['development', 'docker/env/development.env.example'],
    ['test', 'docker/env/test.env.example'],
    ['staging', 'docker/env/staging.env.example'],
    ['production', 'docker/env/production.env.example'],
    ['demo', 'docker/env/demo.env.example'],
    ['development', 'docker/.env.example'],
  ].map(([env, f]) => envFileOf(f, env)),
];

if (ALSO_BUNDLE_GATEWAY) {
  targets.push(
    composeOfFile('../sdkwork-webserver/deployments/docker/docker-compose.bundle-gateway.yml', 'development'),
  );
}

const results = targets.map(processTarget).filter(Boolean);

if (!APPLY) {
  for (const r of results) {
    console.log(`# ${r.file}  (${r.environment})  total=${r.origins.length}  addedConsole=${r.origins.length - r.seedCount}`);
  }
  console.log(
    `Dry run. roles=${WEB_CONSOLE_CLIENT_ROLES.length} baseDomains=${BASE_DOMAINS.length} targets=${results.length}`,
  );
  console.log('Re-run with --apply [--bundle] to rewrite the CORS allow-lists.');
  process.exit(0);
}

let applied = 0;
for (const r of results) {
  const target = targets.find((t) => t.file === r.file);
  if (!target) continue;
  const path = join(REPO_ROOT, r.file);
  const text = readFileSync(path, 'utf8');
  writeFileSync(path, target.replace(text, target.render(r.origins)), 'utf8');
  applied += 1;
  console.log(`applied ${r.file}: ${r.origins.length} allowed origins`);
}
console.log(`Done. ${applied} file(s) rewritten.`);

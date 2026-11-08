#!/usr/bin/env node
/**
 * check-native-column-bindings.mjs
 *
 * Workspace guard for the defect class behind the production 50001 on
 * `POST /app/v3/api/recharges/orders`: a Rust `String`/`&str` bound by sqlx as
 * a PostgreSQL TEXT parameter into a column that is native TIMESTAMPTZ,
 * TIMESTAMP, DATE or NUMERIC without an explicit SQL cast. PostgreSQL rejects
 * such a statement during analysis ("column ... is of type timestamp with time
 * zone but expression is of type text") and the request fails with 500.
 *
 * The checker builds a table/column type catalog from every `database/**\/*.sql`
 * in each repository (baseline + migrations, last definition wins) and, when
 * `--catalog <file>` is supplied, from a `schema|table|column|data_type` dump of
 * the deployed database. It then scans Rust raw SQL strings for INSERT/UPDATE
 * targets and comparisons whose placeholder has no cast.
 *
 * Because the scanner cannot see Rust types, every hit still needs the
 * reviewer's verdict (native `chrono`/`uuid`/`serde_json`/integer bind, dynamic
 * SQL). Treat the output as a review queue, not as a build failure on its own.
 *
 * Usage:
 *   node tools/check-native-column-bindings.mjs --workspace D:\sdkwork-space \
 *        [--catalog prod-columns.txt] [--repo sdkwork-order] [--json]
 *   node tools/check-native-column-bindings.mjs --self-test
 */

import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const SKIP_DIRS = new Set([
  'node_modules', 'target', 'target-win', '.git', 'dist', 'vendor', 'tmp', 'tmp-build',
  'build', '.next', '.turbo', 'coverage', 'generated', 'sdks', 'apps',
]);

const TYPED = new Set(['timestamptz', 'timestamp', 'numeric', 'date']);

function walk(dir, filter, out = [], depth = 0) {
  if (depth > 12) return out;
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const entry of entries) {
    if (SKIP_DIRS.has(entry.name) || entry.name.startsWith('.')) continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, filter, out, depth + 1);
    else if (filter(entry.name)) out.push(full);
  }
  return out;
}

function normalizeType(raw) {
  const t = raw.toLowerCase().replace(/\s+/g, ' ').trim();
  if (/^timestamptz/.test(t)) return 'timestamptz';
  if (/^timestamp /.test(t)) return 'timestamp';
  if (/^numeric|^decimal/.test(t)) return 'numeric';
  if (/^date/.test(t)) return 'date';
  return t;
}

function matchParen(text, openIdx) {
  let depth = 0;
  for (let i = openIdx; i < text.length; i++) {
    if (text[i] === '(') depth++;
    else if (text[i] === ')') {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

function splitTopLevel(s) {
  const out = [];
  let depth = 0;
  let cur = '';
  for (const ch of s) {
    if (ch === '(') depth++;
    if (ch === ')') depth--;
    if (ch === ',' && depth === 0) {
      out.push(cur.trim());
      cur = '';
    } else cur += ch;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

function stripSqlComments(sql) {
  return sql.replace(/--[^\n]*/g, '').replace(/\/\*[\s\S]*?\*\//g, '');
}

export function parseDdl(catalog, sqlText) {
  const clean = stripSqlComments(sqlText);
  const createRe = /CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(?:[a-z0-9_]+\.)?([a-z0-9_]+)\s*\(/gi;
  let m;
  while ((m = createRe.exec(clean))) {
    const table = m[1].toLowerCase();
    const open = m.index + m[0].length - 1;
    const close = matchParen(clean, open);
    if (close < 0) continue;
    for (const part of splitTopLevel(clean.slice(open + 1, close))) {
      const first = part.trim().split(/\s+/);
      if (!first.length) continue;
      const col = first[0].toLowerCase();
      if (['constraint', 'primary', 'unique', 'foreign', 'check', 'exclude'].includes(col)) continue;
      const typeMatch = /^([a-z]+(?:\s+(?:with|without)\s+time\s+zone)?(?:\s+varying)?(?:\s*\([^)]*\))?)/i.exec(
        part.trim().slice(col.length).trim(),
      );
      if (typeMatch) catalog.set(`${table}.${col}`, normalizeType(typeMatch[1]));
    }
  }
  const addRe = /ALTER\s+TABLE\s+(?:[a-z0-9_]+\.)?([a-z0-9_]+)\s+ADD\s+COLUMN\s+(?:IF\s+NOT\s+EXISTS\s+)?([a-z0-9_]+)\s+([a-z]+(?:\s+(?:with|without)\s+time\s+zone)?(?:\s+varying)?(?:\s*\([^)]*\))?)/gi;
  while ((m = addRe.exec(clean))) catalog.set(`${m[1].toLowerCase()}.${m[2].toLowerCase()}`, normalizeType(m[3]));
  const alterRe = /ALTER\s+TABLE\s+(?:[a-z0-9_]+\.)?([a-z0-9_]+)\s+ALTER\s+COLUMN\s+([a-z0-9_]+)\s+(?:SET\s+DATA\s+)?TYPE\s+([a-z]+(?:\s+(?:with|without)\s+time\s+zone)?(?:\s+varying)?(?:\s*\([^)]*\))?)/gi;
  while ((m = alterRe.exec(clean))) catalog.set(`${m[1].toLowerCase()}.${m[2].toLowerCase()}`, normalizeType(m[3]));
}

function extractSqlLiterals(text) {
  const out = [];
  const rawRe = /r(#*)"([\s\S]*?)"\1/g;
  let m;
  while ((m = rawRe.exec(text))) {
    if (/\b(INSERT\s+INTO|UPDATE)\b/i.test(m[2])) {
      out.push({ sql: m[2], line: text.slice(0, m.index).split('\n').length, end: m.index + m[0].length });
    }
  }
  return out;
}

function bindsAfter(text, fromIdx) {
  const out = [];
  let i = fromIdx;
  while (true) {
    const match = /\.bind(?:_\w+)?\(/.exec(text.slice(i, i + 500));
    if (!match) break;
    const b = i + match.index;
    const nextQuery = text.indexOf('sqlx::query', i);
    if (nextQuery >= 0 && nextQuery < b) break;
    const open = b + match[0].length - 1;
    const close = matchParen(text, open);
    if (close < 0) break;
    out.push(text.slice(open + 1, close).trim().replace(/\s+/g, ' '));
    i = close + 1;
    if (!/^\s*\.bind/.test(text.slice(i, i + 20))) break;
  }
  return out;
}

function bareParams(expr) {
  let s = expr.replace(/CAST\s*\(\s*\$\d+\s+AS\s+[A-Z0-9_()., ]+?\s*\)/gi, ' ');
  s = s.replace(/\$\d+\s*::\s*[a-z0-9_ ]+/gi, ' ');
  return [...s.matchAll(/\$(\d+)/g)].map((x) => Number(x[1]));
}

function searchKeyword(sql, words, from) {
  let best = -1;
  for (const w of words) {
    const at = from + sql.slice(from).search(new RegExp(`\\b${w}\\b`, 'i'));
    if (at === from - 1) continue;
    if (best < 0 || at < best) best = at;
  }
  return best;
}

export function scanSql(sql, catalog, report) {
  const clean = stripSqlComments(sql);
  const insertRe = /\bINSERT\s+INTO\s+(?:[a-z0-9_]+\.)?([a-z0-9_]+)\s*\(/gi;
  let m;
  while ((m = insertRe.exec(clean))) {
    const table = m[1].toLowerCase();
    const colClose = matchParen(clean, m.index + m[0].length - 1);
    if (colClose < 0) continue;
    const cols = splitTopLevel(clean.slice(m.index + m[0].length, colClose)).map((c) => c.trim().toLowerCase());
    const valuesIdx = clean.indexOf('VALUES', colClose);
    if (valuesIdx < 0 || valuesIdx - colClose > 200) continue;
    let q = valuesIdx + 6;
    while (/\s/.test(clean[q])) q++;
    if (clean[q] !== '(') continue;
    const valsClose = matchParen(clean, q);
    if (valsClose < 0) continue;
    const vals = splitTopLevel(clean.slice(q + 1, valsClose));
    if (vals.length !== cols.length) continue;
    cols.forEach((col, i) => {
      const type = catalog.get(`${table}.${col}`);
      if (!type || !TYPED.has(type)) return;
      for (const n of bareParams(vals[i])) report('INSERT', table, col, type, n);
    });
  }

  const updateRe = /\bUPDATE\s+(?:[a-z0-9_]+\.)?([a-z0-9_]+)\s+SET\b/gi;
  while ((m = updateRe.exec(clean))) {
    const table = m[1].toLowerCase();
    const end = searchKeyword(clean, ['where', 'from', 'returning'], m.index + m[0].length);
    const assignments = splitTopLevel(clean.slice(m.index + m[0].length, end < 0 ? clean.length : end));
    for (const a of assignments) {
      const eq = a.indexOf('=');
      if (eq < 0) continue;
      const col = a.slice(0, eq).trim().toLowerCase();
      const type = catalog.get(`${table}.${col}`);
      if (!type || !TYPED.has(type)) continue;
      for (const n of bareParams(a.slice(eq + 1))) report('UPDATE', table, col, type, n);
    }
  }
}

function runSelfTest() {
  const catalog = new Map();
  parseDdl(catalog, 'CREATE TABLE t (id TEXT PRIMARY KEY, created_at TIMESTAMPTZ NOT NULL, amount NUMERIC(20,0));');
  const findings = [];
  scanSql(
    'INSERT INTO t (id, created_at, amount) VALUES ($1, $2, $3)',
    catalog,
    (kind, table, col, type) => findings.push(`${kind}:${table}.${col}:${type}`),
  );
  scanSql(
    'INSERT INTO t (id, created_at, amount) VALUES ($1, CAST($2 AS TIMESTAMPTZ), $3::numeric)',
    catalog,
    (kind, table, col, type) => findings.push(`UNEXPECTED:${table}.${col}:${type}`),
  );
  const ok = findings.length === 2 && findings.includes('INSERT:t.created_at:timestamptz')
    && findings.includes('INSERT:t.amount:numeric')
    && !findings.some((f) => f.startsWith('UNEXPECTED'));
  if (!ok) {
    console.error('self-test FAILED:', findings);
    process.exit(1);
  }
  console.log('self-test OK');
}

function parseArgs(argv) {
  const args = { workspace: null, catalog: null, repo: null, json: false, selfTest: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--self-test') args.selfTest = true;
    else if (a === '--json') args.json = true;
    else if (a === '--workspace') args.workspace = argv[++i];
    else if (a === '--catalog') args.catalog = argv[++i];
    else if (a === '--repo') args.repo = argv[++i];
    else if (a === '--help' || a === '-h') args.help = true;
  }
  return args;
}

function usage() {
  console.log(`Usage:
  node tools/check-native-column-bindings.mjs --workspace <dir> [--catalog <tsv>] [--repo <name>] [--json]
  node tools/check-native-column-bindings.mjs --self-test

Checks Rust raw SQL for text-bound parameters targeting native TIMESTAMPTZ /
TIMESTAMP / DATE / NUMERIC columns without an explicit cast. Output is a review
queue: confirm the Rust type before changing anything.`);
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.selfTest) return runSelfTest();
  if (args.help || !args.workspace) {
    usage();
    process.exit(args.help ? 0 : 2);
  }

  const workspace = path.resolve(args.workspace);
  const prod = new Map();
  if (args.catalog) {
    for (const line of readFileSync(path.resolve(args.catalog), 'utf8').split(/\r?\n/)) {
      const parts = line.split('|');
      if (parts.length < 4) continue;
      prod.set(`${parts[1].toLowerCase()}.${parts[2].toLowerCase()}`, normalizeType(parts[3]));
    }
  }

  const repos = readdirSync(workspace, { withFileTypes: true })
    .filter((e) => e.isDirectory() && !e.name.startsWith('.'))
    .map((e) => e.name)
    .filter((name) => !args.repo || name === args.repo);

  const rows = [];
  for (const repo of repos) {
    const root = path.join(workspace, repo);
    if (!statSync(root).isDirectory()) continue;
    const catalog = new Map(prod);
    const sqlFiles = walk(root, (n) => n.endsWith('.sql')).sort((a, b) => a.localeCompare(b));
    for (const file of sqlFiles) {
      try {
        parseDdl(catalog, readFileSync(file, 'utf8'));
      } catch {
        /* unreadable */
      }
    }
    for (const file of walk(root, (n) => n.endsWith('.rs'))) {
      let text;
      try {
        text = readFileSync(file, 'utf8');
      } catch {
        continue;
      }
      for (const lit of extractSqlLiterals(text)) {
        const binds = bindsAfter(text, lit.end);
        scanSql(lit.sql, catalog, (kind, table, col, type, n) => {
          rows.push({
            repo,
            file: path.relative(workspace, file),
            line: lit.line,
            kind,
            column: `${table}.${col}`,
            type,
            bind: binds[n - 1] || '',
          });
        });
      }
    }
  }

  if (args.json) {
    console.log(JSON.stringify({ findings: rows.length, rows }, null, 2));
  } else {
    for (const r of rows) {
      console.log(`${r.repo}\t${r.file}:${r.line}\t${r.kind}\t${r.column}\t${r.type}\t${r.bind}`);
    }
    console.log(`\nfindings: ${rows.length} (review queue; confirm the Rust type before fixing)`);
  }
  process.exit(rows.length ? 1 : 0);
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) main();

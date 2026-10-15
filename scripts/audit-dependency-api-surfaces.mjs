#!/usr/bin/env node
// Audit the gateway's declared dependency API surfaces against the route
// literals found in the dependency workspaces' generated HTTP route manifests.
//
// The gateway refuses to start when an embedded assembly serves a route that no
// declared dependency surface covers (`runtime.rs`: "route ... is not declared
// by a matching dependency API surface"). Because that check runs during
// composition, discovering gaps one boot cycle at a time is slow.
//
// NOTE - this is a navigational report, not a gate. It scans route literals in
// files whose name contains "manifest", so it also reports non-route literals
// (health probes) and partial prefixes, and it cannot know which of a
// workspace's several service ids owns a path. Treat its output as candidate
// families to review, and author the real declaration from the owner's own
// route manifest.
//
// Usage: node scripts/audit-dependency-api-surfaces.mjs [--workspace <dir>] [--json]

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function parseArgs(args) {
  let workspaceRoot = path.resolve(repoRoot, '..');
  let asJson = false;
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === '--workspace') {
      workspaceRoot = path.resolve(args[index + 1]);
      index += 1;
    } else if (args[index] === '--json') {
      asJson = true;
    }
  }
  return { workspaceRoot, asJson };
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

/** Longest-prefix match, mirroring `route_path_belongs_to_prefix` in runtime.rs. */
function surfaceCovers(prefix, routePath) {
  if (routePath === prefix) {
    return true;
  }
  return routePath.startsWith(prefix.endsWith('/') ? prefix : `${prefix}/`);
}

function collectManifestRoutes(directory, routes) {
  let entries;
  try {
    entries = fs.readdirSync(directory, { withFileTypes: true });
  } catch {
    return;
  }
  for (const entry of entries) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'target' || entry.name === 'node_modules') {
        continue;
      }
      collectManifestRoutes(entryPath, routes);
      continue;
    }
    if (!entry.name.endsWith('.rs') || !entry.name.includes('manifest')) {
      continue;
    }
    let source;
    try {
      source = fs.readFileSync(entryPath, 'utf8');
    } catch {
      continue;
    }
    // Route literals are the quoted absolute paths in an HttpRoute table.
    for (const match of source.matchAll(/"(\/[A-Za-z0-9_\-./{}]*)"/gu)) {
      const routePath = match[1];
      if (routePath.startsWith('/') && routePath.length > 1) {
        routes.add(routePath);
      }
    }
  }
}

function routesForWorkspace(workspaceRoot, workspace) {
  const root = path.join(workspaceRoot, workspace);
  const routes = new Set();
  collectManifestRoutes(root, routes);
  return routes;
}

function main() {
  const { workspaceRoot, asJson } = parseArgs(process.argv.slice(2));
  const spec = readJson(path.join(repoRoot, 'specs', 'component.spec.json'));
  const surfaces = spec.apiSurfaces.filter(
    (surface) => String(surface.ownership ?? '').toLowerCase() === 'dependency',
  );
  const byPrefix = new Map();
  for (const surface of surfaces) {
    byPrefix.set(surface.prefix, surface.dependencyServiceId);
  }
  const prefixes = [...byPrefix.keys()].sort((left, right) => right.length - left.length);

  const byWorkspace = new Map();
  for (const dependency of spec.dependencies ?? []) {
    if (!byWorkspace.has(dependency.workspace)) {
      byWorkspace.set(dependency.workspace, []);
    }
    byWorkspace.get(dependency.workspace).push(dependency.serviceId);
  }

  const findings = [];
  for (const [workspace, serviceIds] of byWorkspace) {
    const routes = routesForWorkspace(workspaceRoot, workspace);
    if (routes.size === 0) {
      continue;
    }
    const uncovered = new Map();
    for (const routePath of routes) {
      const covered = prefixes.some((prefix) => surfaceCovers(prefix, routePath));
      if (!covered) {
        const topLevel = `/${routePath.split('/')[1]}`;
        if (!uncovered.has(topLevel)) {
          uncovered.set(topLevel, []);
        }
        uncovered.get(topLevel).push(routePath);
      }
    }
    for (const [topLevel, exampleRoutes] of uncovered) {
      findings.push({
        workspace,
        serviceIds,
        topLevel,
        exampleRoutes: exampleRoutes.sort().slice(0, 3),
        routeCount: exampleRoutes.length,
      });
    }
  }

  findings.sort((left, right) => (
    left.workspace.localeCompare(right.workspace) || left.topLevel.localeCompare(right.topLevel)
  ));

  if (asJson) {
    process.stdout.write(`${JSON.stringify({ declaredSurfaceCount: prefixes.length, findings }, null, 2)}\n`);
    return;
  }
  process.stdout.write(
    `declared dependency surfaces: ${prefixes.length}\n`
    + `undeclared route families: ${findings.length}\n`,
  );
  for (const finding of findings) {
    process.stdout.write(
      `\n${finding.workspace} (${finding.serviceIds.join(', ')})\n`
      + `  prefix to declare: ${finding.topLevel}\n`
      + `  routes: ${finding.routeCount}\n`
      + `  examples: ${finding.exampleRoutes.join(', ')}\n`,
    );
  }
}

main();

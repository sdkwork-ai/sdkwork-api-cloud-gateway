#!/usr/bin/env node
/**
 * Reclaim runner disk before a large release build (CI only).
 *
 * The gateway release build links 200+ workspace crates through the embedded
 * foundation assemblies and exceeds the headroom of a GitHub-hosted runner: the
 * linux package job failed with
 *
 *   ENOSPC: no space left on device, copyfile
 *   target/release/sdkwork-api-cloud-gateway ->
 *   dist/server/.../bin/sdkwork-api-cloud-gateway
 *
 * The runner image ships SDKs this build never uses (Android, .NET, GHC, Chrome,
 * CodeQL). Removing them before the build restores that headroom. The toolchains
 * the build depends on — Rust, Node, pnpm, Docker, the PostgreSQL client — are
 * never touched, and nothing is removed outside CI unless --apply is passed.
 *
 * Usage: node scripts/ci/reclaim-runner-disk.mjs [--apply]
 */
import { execFileSync } from 'node:child_process';
import { existsSync, rmSync } from 'node:fs';
import process from 'node:process';

/** Preinstalled toolchains that a Rust/Docker release build does not need. */
const RECLAIMABLE_PATHS = [
  '/usr/share/dotnet',
  '/usr/local/lib/android',
  '/opt/ghc',
  '/usr/local/.ghcup',
  '/opt/hostedtoolcache/CodeQL',
  '/usr/local/share/powershell',
  '/usr/local/share/chromium',
  '/opt/google/chrome',
  '/usr/lib/firefox',
];

function availableGigabytes() {
  try {
    const output = execFileSync('df', ['-Pk', '/'], { encoding: 'utf8' });
    const line = output.trim().split('\n').at(-1) ?? '';
    const availableKb = Number(line.split(/\s+/u)[3]);
    return Number.isFinite(availableKb) ? availableKb / 1024 ** 2 : null;
  } catch {
    return null;
  }
}

function report(prefix) {
  const gigabytes = availableGigabytes();
  console.log(
    `[reclaim-runner-disk] ${prefix}: ${gigabytes === null ? 'unknown' : `${gigabytes.toFixed(2)} GB free`}`,
  );
}

function main(argv = process.argv.slice(2)) {
  const apply = argv.includes('--apply');
  if (process.platform !== 'linux') {
    console.log(`[reclaim-runner-disk] skipping: not a linux runner (${process.platform})`);
    return 0;
  }
  if (!process.env.CI && !process.env.GITHUB_ACTIONS && !apply) {
    console.log('[reclaim-runner-disk] skipping: CI environment not detected (pass --apply to force)');
    return 0;
  }

  report('before');
  let removed = 0;
  for (const target of RECLAIMABLE_PATHS) {
    if (!existsSync(target)) continue;
    console.log(`[reclaim-runner-disk] removing ${target}`);
    try {
      rmSync(target, { recursive: true, force: true });
      removed += 1;
    } catch (error) {
      console.log(`[reclaim-runner-disk] could not remove ${target}: ${error.message}`);
    }
  }
  console.log(`[reclaim-runner-disk] removed ${removed} path(s)`);
  report('after');
  return 0;
}

if (process.argv[1] && process.argv[1].endsWith('reclaim-runner-disk.mjs')) {
  process.exitCode = main();
}

export { main, RECLAIMABLE_PATHS };

#!/usr/bin/env node
/**
 * Install the native build dependencies the release link needs on this host (CI).
 *
 * The gateway links the foundation suite, which reaches `audiopus_sys`. That
 * crate either finds a system Opus or builds Opus from source, and building it
 * needs autotools/cmake. Windows hosts use the repository's prebuilt path
 * (`pnpm build:native-deps:windows`); macOS runners ship neither autotools nor a
 * system Opus, so the build failed there with:
 *
 *   error: failed to run custom build command for `audiopus_sys v0.1.8`
 *   Failed to autogen Opus.
 *
 * This step installs the missing build-time tools on macOS. Linux and Windows
 * hosts already provide what they need, so it logs a skip.
 *
 * Usage: node scripts/ci/install-build-dependencies.mjs
 */
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import process from 'node:process';

/** Tools required to autogen and build the vendored Opus sources. */
const MACOS_TOOLS = ['cmake', 'autoconf', 'automake', 'libtool'];

/** Homebrew prefixes to search before running `brew install`. */
const MACOS_TOOL_PATHS = {
  cmake: ['/opt/homebrew/bin/cmake', '/usr/local/bin/cmake'],
  autoconf: ['/opt/homebrew/bin/autoconf', '/usr/local/bin/autoconf'],
  automake: ['/opt/homebrew/bin/automake', '/usr/local/bin/automake'],
  libtool: ['/opt/homebrew/bin/libtool', '/usr/local/bin/libtool'],
};

function present(tool) {
  try {
    execFileSync('which', [tool], { stdio: 'ignore' });
    return true;
  } catch {
    return (MACOS_TOOL_PATHS[tool] ?? []).some((candidate) => existsSync(candidate));
  }
}

function brewPrefix() {
  for (const candidate of ['/opt/homebrew/bin/brew', '/usr/local/bin/brew']) {
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

function main() {
  if (process.platform !== 'darwin') {
    console.log(`[install-build-dependencies] skipping: no extra tools required on ${process.platform}`);
    return 0;
  }

  const missing = MACOS_TOOLS.filter((tool) => !present(tool));
  if (missing.length === 0) {
    console.log('[install-build-dependencies] all build tools already present');
    return 0;
  }

  const brew = brewPrefix();
  if (!brew) {
    console.error(
      `[install-build-dependencies] Homebrew not found; cannot install ${missing.join(', ')}`,
    );
    return 1;
  }

  console.log(`[install-build-dependencies] installing via Homebrew: ${missing.join(' ')}`);
  try {
    // `libtool` on macOS is `libtool` (a separate formula from the Xcode shim).
    execFileSync(brew, ['install', ...missing], {
      stdio: 'inherit',
      timeout: 20 * 60 * 1000,
      env: { ...process.env, HOMEBREW_NO_AUTO_UPDATE: '1', HOMEBREW_NO_INSTALL_CLEANUP: '1' },
    });
  } catch (error) {
    console.error(`[install-build-dependencies] brew install failed: ${error.message}`);
    return 1;
  }

  const stillMissing = MACOS_TOOLS.filter((tool) => !present(tool));
  if (stillMissing.length > 0) {
    console.error(
      `[install-build-dependencies] still missing after install: ${stillMissing.join(', ')}`,
    );
    return 1;
  }
  console.log('[install-build-dependencies] build tools ready');
  return 0;
}

if (process.argv[1] && process.argv[1].endsWith('install-build-dependencies.mjs')) {
  process.exitCode = main();
}

export { main, MACOS_TOOLS };

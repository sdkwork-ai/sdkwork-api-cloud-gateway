#!/usr/bin/env node
/**
 * Install the native build dependencies the release link needs on this host (CI).
 *
 * The gateway links the foundation suite, which reaches `audiopus_sys`. That
 * crate either finds a system Opus or builds Opus from source, and building it
 * needs autotools/cmake. Its documented escape hatch is
 * `LIBOPUS_LIB_DIR`/`OPUS_LIB_DIR`, which links a prebuilt library and skips
 * the source build entirely.
 *
 * Two hosts need help, for different reasons:
 *
 * - macOS runners ship neither autotools nor a system Opus, so the vendored
 *   source build failed there with:
 *
 *     error: failed to run custom build command for `audiopus_sys v0.1.8`
 *     Failed to autogen Opus.
 *
 *   Installing the autotools/cmake toolchain lets that build proceed.
 *
 * - Windows fails earlier and differently. `audiopus_sys` reaches its
 *   Unix-only source path (`cp`, `sh autogen.sh`, `make`), and Git Bash's MSYS
 *   `cp` strips the backslashes out of the `\\?\C:\...` verbatim path it is
 *   handed, so the copy never happens:
 *
 *     cargo:info=`pkg_config` could not find `Opus`.
 *     cargo:info=Opus will be built as static-library.
 *     cp: cannot stat '\?C:Users...audiopus_sys-0.1.8opus': No such file or directory
 *     thread 'main' panicked at audiopus_sys-0.1.8/build.rs:90:
 *     Failed to copy Opus files.
 *
 *   That source path cannot succeed on Windows at any toolchain level, so this
 *   script instead builds `libopus.a` from the same vendored sources with
 *   MinGW gcc/ar + CMake (scripts/build-windows-libopus.ps1) and exports
 *   `LIBOPUS_LIB_DIR` for the rest of the job.
 *
 * Linux runners already provide Opus through pkg-config, so they log a skip.
 *
 * Usage: node scripts/ci/install-build-dependencies.mjs
 */
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
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

function installMacOsBuildTools() {
  const missing = MACOS_TOOLS.filter((tool) => !present(tool));
  if (missing.length === 0) {
    console.log('[install-build-dependencies] all macOS build tools already present');
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
  console.log('[install-build-dependencies] macOS build tools ready');
  return 0;
}

/** Relative location of the prebuilt libopus the Windows script produces. */
const PREBUILT_OPUS_LIB_DIR = 'target/prebuilt-opus/lib';

/**
 * Builds the prebuilt Windows Opus archive and exports `LIBOPUS_LIB_DIR` so the
 * `audiopus_sys` build script links it instead of taking its Unix-only source
 * path. The export only affects this process and its children, so the build step
 * that runs afterwards must inherit it: the caller has to run this script and
 * the build in the same shell, which `sdkwork.workflow.json` does by invoking
 * it from the same lifecycle `run` block.
 */
function installWindowsPrebuiltOpus() {
  const repositoryRoot = path.resolve(import.meta.dirname, '..', '..');
  const scriptPath = path.join(repositoryRoot, 'scripts', 'build-windows-libopus.ps1');
  if (!existsSync(scriptPath)) {
    console.error(`[install-build-dependencies] missing ${scriptPath}`);
    return 1;
  }

  // PowerShell 7 (`pwsh`) is the documented interpreter, but GitHub's windows
  // runners only guarantee Windows PowerShell 5.1, so fall back to it. The
  // script stays within the 5.1-compatible subset.
  const candidates = [
    { command: 'pwsh', prefixArgs: ['-NoProfile', '-ExecutionPolicy', 'Bypass'] },
    { command: 'powershell.exe', prefixArgs: ['-NoProfile', '-ExecutionPolicy', 'Bypass'] },
  ].filter((candidate) => present(candidate.command));

  if (candidates.length === 0) {
    console.error('[install-build-dependencies] neither pwsh nor powershell.exe is available');
    return 1;
  }

  const shell = candidates[0];
  console.log(`[install-build-dependencies] building prebuilt Opus with ${shell.command}`);
  try {
    execFileSync(
      shell.command,
      [...shell.prefixArgs, '-File', scriptPath],
      {
        stdio: 'inherit',
        cwd: repositoryRoot,
        timeout: 30 * 60 * 1000,
      },
    );
  } catch (error) {
    console.error(`[install-build-dependencies] prebuilt Opus build failed: ${error.message}`);
    return 1;
  }

  const libraryPath = path.join(repositoryRoot, PREBUILT_OPUS_LIB_DIR, 'libopus.a');
  if (!existsSync(libraryPath)) {
    console.error(`[install-build-dependencies] prebuilt Opus library missing: ${libraryPath}`);
    return 1;
  }

  process.env.LIBOPUS_LIB_DIR = path.join(repositoryRoot, PREBUILT_OPUS_LIB_DIR);
  console.log(
    `[install-build-dependencies] LIBOPUS_LIB_DIR=${process.env.LIBOPUS_LIB_DIR}`,
  );
  return 0;
}

function main() {
  if (process.platform === 'win32') {
    return installWindowsPrebuiltOpus();
  }

  if (process.platform === 'darwin') {
    return installMacOsBuildTools();
  }

  console.log(
    `[install-build-dependencies] skipping: pkg-config resolves Opus on ${process.platform}`,
  );
  return 0;
}

if (process.argv[1] && process.argv[1].endsWith('install-build-dependencies.mjs')) {
  process.exitCode = main();
}

export { main, installMacOsBuildTools, installWindowsPrebuiltOpus, MACOS_TOOLS };

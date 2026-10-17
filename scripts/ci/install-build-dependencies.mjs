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
 *   Installing the autotools/cmake toolchain lets that build proceed. The
 *   `libtool` half of that toolchain is easy to get wrong: macOS ships Apple's
 *   unrelated `/usr/bin/libtool`, so probing the `libtool` name reported the
 *   tool as present and Homebrew's GNU libtool was never installed. Without
 *   `libtool.m4` on aclocal's path, Opus's autogen still died with:
 *
 *     configure.ac:38: error: undefined or overquoted macro: AM_PROG_LIBTOOL
 *
 *   This script therefore probes GNU libtool's own executables, exports the
 *   macro/libtoolize environment the build needs, and proves the recipe with a
 *   real `autoreconf -i` run before the long build starts.
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
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';

/** Tools required to autogen and build the vendored Opus sources. */
const MACOS_TOOLS = ['cmake', 'autoconf', 'automake', 'libtool'];

/**
 * Executables that prove the tool the build actually needs is installed.
 *
 * macOS ships Apple's `/usr/bin/libtool`, which shares a name with GNU libtool
 * but provides none of the autoconf macros Opus needs. That name collision is
 * what made an earlier run install autoconf and automake only, leaving
 * `libtool.m4` missing. GNU libtool is the package that ships
 * `glibtoolize`/`libtoolize`, so those are the probes.
 */
const MACOS_TOOL_PROBES = {
  libtool: ['glibtoolize', 'libtoolize'],
};

/** Homebrew prefixes to search before running `brew install`. */
const MACOS_TOOL_PATHS = {
  cmake: ['/opt/homebrew/bin/cmake', '/usr/local/bin/cmake'],
  autoconf: ['/opt/homebrew/bin/autoconf', '/usr/local/bin/autoconf'],
  automake: ['/opt/homebrew/bin/automake', '/usr/local/bin/automake'],
  libtool: [
    '/opt/homebrew/bin/glibtoolize',
    '/usr/local/bin/glibtoolize',
    '/opt/homebrew/bin/libtoolize',
    '/usr/local/bin/libtoolize',
  ],
};

/** Prefixes whose `share/aclocal` holds GNU libtool's m4 macros. */
const MACOS_ACLOCAL_DIRS = ['/opt/homebrew/share/aclocal', '/usr/local/share/aclocal'];

/** GNU libtoolize entry points, in Homebrew naming order. */
const MACOS_LIBTOOLIZE_PATHS = ['/opt/homebrew/bin/glibtoolize', '/usr/local/bin/glibtoolize'];

function commandExists(command) {
  try {
    execFileSync('which', [command], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

function present(tool) {
  const probes = MACOS_TOOL_PROBES[tool] ?? [tool];
  if (probes.some((probe) => commandExists(probe))) return true;
  return (MACOS_TOOL_PATHS[tool] ?? []).some((candidate) => existsSync(candidate));
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
  } else {
    const brew = brewPrefix();
    if (!brew) {
      console.error(
        `[install-build-dependencies] Homebrew not found; cannot install ${missing.join(', ')}`,
      );
      return 1;
    }

    console.log(`[install-build-dependencies] installing via Homebrew: ${missing.join(' ')}`);
    try {
      // `libtool` here is GNU libtool (glibtoolize), not Apple's Xcode shim.
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
  }

  return configureMacOsLibtoolEnvironment();
}

/** aclocal's macro directory as the installed automake sees it. */
function aclocalMacroDir() {
  try {
    return execFileSync('aclocal', ['--print-ac-dir'], { encoding: 'utf8' }).trim();
  } catch {
    return '';
  }
}

/**
 * Export the GNU libtool environment Opus's autogen needs, then prove it works.
 *
 * `audiopus_sys` shells out to `autoreconf -i`, which resolves `libtool.m4`
 * through aclocal and invokes libtoolize. Homebrew installs both under names
 * aclocal and autoreconf only find when told where to look, so the paths are
 * exported here. The export lives in this process, so the caller must run the
 * build in the same shell - both the build and package lifecycle blocks in
 * `sdkwork.workflow.json` do.
 *
 * The probe is the point: it runs the same `autoreconf -i` recipe against a
 * throwaway `configure.ac` that uses `AM_PROG_LIBTOOL`, so a broken toolchain
 * fails in seconds instead of surfacing twenty minutes into the package step.
 */
function configureMacOsLibtoolEnvironment() {
  const macroDirs = [...new Set([aclocalMacroDir(), ...MACOS_ACLOCAL_DIRS])]
    .filter((dir) => dir && existsSync(path.join(dir, 'libtool.m4')));
  if (macroDirs.length === 0) {
    console.error(
      '[install-build-dependencies] GNU libtool is installed but no libtool.m4 was found in ' +
        `${MACOS_ACLOCAL_DIRS.join(', ')}; Opus autogen would fail with AM_PROG_LIBTOOL undefined`,
    );
    return 1;
  }

  const aclocalPath = (process.env.ACLOCAL_PATH ?? '').split(':').filter(Boolean);
  process.env.ACLOCAL_PATH = [...new Set([...aclocalPath, ...macroDirs])].join(':');
  console.log(`[install-build-dependencies] ACLOCAL_PATH=${process.env.ACLOCAL_PATH}`);

  const libtoolize = MACOS_LIBTOOLIZE_PATHS.find((candidate) => existsSync(candidate));
  if (libtoolize) {
    process.env.LIBTOOLIZE = libtoolize;
    console.log(`[install-build-dependencies] LIBTOOLIZE=${libtoolize}`);
  }

  const probe = probeAutoreconfLibtool();
  if (!probe.ok) {
    console.error(
      '[install-build-dependencies] autoreconf probe failed; Opus autogen would fail the same way',
    );
    console.error(probe.detail);
    return 1;
  }

  console.log('[install-build-dependencies] autoreconf + AM_PROG_LIBTOOL probe passed');
  return 0;
}

/**
 * Runs the autogen recipe `audiopus_sys` uses against a minimal `configure.ac`
 * that declares `AM_PROG_LIBTOOL`, so the toolchain is proven before the long
 * release build depends on it.
 */
function probeAutoreconfLibtool() {
  const probeDir = mkdtempSync(path.join(tmpdir(), 'sdkwork-libtool-probe-'));
  try {
    writeFileSync(
      path.join(probeDir, 'configure.ac'),
      'AC_INIT([sdkwork-libtool-probe], [0.0.0])\nAM_PROG_LIBTOOL\nAC_OUTPUT\n',
    );
    execFileSync('autoreconf', ['-i'], {
      cwd: probeDir,
      stdio: 'pipe',
      timeout: 5 * 60 * 1000,
      env: process.env,
    });
    return { ok: true, detail: '' };
  } catch (error) {
    const output = [error.stdout?.toString(), error.stderr?.toString()]
      .filter(Boolean)
      .join('\n')
      .trim();
    return { ok: false, detail: output.slice(-2000) || error.message };
  } finally {
    rmSync(probeDir, { recursive: true, force: true });
  }
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

export {
  main,
  installMacOsBuildTools,
  installWindowsPrebuiltOpus,
  configureMacOsLibtoolEnvironment,
  probeAutoreconfLibtool,
  MACOS_TOOLS,
};

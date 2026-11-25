/**
 * Resolve the Cargo target directory for release packaging.
 *
 * Why this exists
 * ---------------
 * The container image and the host-native server package are Linux artifacts,
 * so both release builds run inside WSL. This repository lives on a Windows
 * drive, which WSL exposes through a 9p (`v9fs`) passthrough mount. That mount
 * is fine for source files and catastrophic for Cargo: a release build of this
 * workspace compiles 1,400+ crates and performs hundreds of thousands of small
 * file operations inside `target/`.
 *
 * Measured on this workstation (16 logical cores, WSL 15 GB):
 *   - walking 4,133 files:      9p ~3.4-4.1 s   vs ext4 ~17-43 ms   (~200x)
 *   - creating 5,000 files:     9p ~22.4 s      vs ext4 ~0.12 s     (~180x)
 *   - sequential 256 MiB read:  9p ~1,436 ms    vs ext4 ~38 ms
 *
 * So when the application root sits on a Windows passthrough filesystem and the
 * caller did not pin `CARGO_TARGET_DIR`, packaging relocates the target
 * directory to the guest's own (ext4) cache root. Everything else — the
 * repository, `dist/`, the staged package — stays where it is.
 *
 * Precedence:
 *   1. an explicit `CARGO_TARGET_DIR` (absolute or repository-relative) always
 *      wins, so CI and developers keep full control;
 *   2. non-Linux hosts keep the historical `<appRoot>/target`;
 *   3. Linux hosts on a native filesystem keep `<appRoot>/target`;
 *   4. Linux hosts on a Windows passthrough mount relocate to
 *      `<cacheRoot>/sdkwork/cargo-target/<appId>`.
 *
 * `resolveCargoTargetDir()` in `package-server.mjs` keeps its pure
 * "resolve what the caller asked for" semantics; this module decides *what the
 * caller should ask for*.
 */

import { spawnSync } from 'node:child_process';
import { homedir } from 'node:os';
import path from 'node:path';
import process from 'node:process';

/**
 * Filesystem types that mean "this path is served by the Windows host through
 * the WSL passthrough layer" rather than by a native Linux filesystem.
 *
 * `ext2/ext3` (what `stat -f` reports for ext4) and `overlay`, `xfs`, `btrfs`,
 * `zfs`, `tmpfs` are deliberately absent: those are native and must not trigger
 * a relocation.
 */
export const WINDOWS_PASSTHROUGH_FILESYSTEM_TYPES = Object.freeze([
  '9p',
  'v9fs',
  'drvfs',
  'cifs',
  'smb3',
  'ntfs3',
  'fuseblk',
]);

/** Cache directory name used under the guest's cache root. */
export const RELOCATED_TARGET_NAMESPACE = Object.freeze(['sdkwork', 'cargo-target']);

/**
 * Whether a filesystem type name identifies a Windows passthrough mount.
 *
 * @param {string | null | undefined} filesystemType Value reported by
 *   `stat -f -c %T` (for example `v9fs`, `ext2/ext3`, `overlay`).
 * @returns {boolean} True when the mount is a Windows passthrough.
 */
export function isWindowsPassthroughFilesystem(filesystemType) {
  const normalized = String(filesystemType ?? '').trim().toLowerCase();
  if (!normalized) {
    return false;
  }
  return WINDOWS_PASSTHROUGH_FILESYSTEM_TYPES.some(
    (candidate) => normalized === candidate || normalized.includes(candidate),
  );
}

/**
 * Probe the filesystem type backing a path.
 *
 * Returns an empty string when the probe is unavailable (non-POSIX host, no
 * coreutils, path missing). An unknown filesystem is never treated as a
 * passthrough mount: failing closed keeps the historical layout rather than
 * silently moving a developer's artifacts.
 *
 * @param {string} root Absolute path to probe.
 * @param {typeof spawnSync} [spawn] Injectable process runner, for tests.
 * @returns {string} Filesystem type, or `''` when unknown.
 */
export function probeFilesystemType(root, spawn = spawnSync) {
  if (!root) {
    return '';
  }
  const result = spawn('stat', ['-f', '-c', '%T', root], {
    encoding: 'utf8',
    windowsHide: true,
  });
  if (!result || result.error || result.status !== 0) {
    return '';
  }
  return String(result.stdout ?? '').trim();
}

/**
 * Resolve the guest cache root that hosts a relocated target directory.
 *
 * Honours `XDG_CACHE_HOME` only when it is absolute, per the XDG base
 * directory specification; a relative value is ignored because the result must
 * be independent of the current working directory.
 *
 * @param {{ env?: Record<string, string | undefined>, homeDir?: string }} [options]
 * @returns {string} Absolute cache root.
 */
export function resolveCacheRoot({ env = process.env, homeDir = homedir() } = {}) {
  const xdgCacheHome = String(env.XDG_CACHE_HOME ?? '').trim();
  if (xdgCacheHome && path.isAbsolute(xdgCacheHome)) {
    return path.normalize(xdgCacheHome);
  }
  const home = String(homeDir ?? '').trim();
  if (!home) {
    throw new Error('cannot resolve a cache root: neither XDG_CACHE_HOME nor a home directory is available');
  }
  return path.join(home, '.cache');
}

/**
 * Resolve the Cargo target directory a packaging command should use.
 *
 * @param {object} options
 * @param {string} options.appRoot Absolute application root.
 * @param {string} options.appId Application id used as the cache leaf.
 * @param {Record<string, string | undefined>} [options.env] Environment to read.
 * @param {string} [options.platform] `process.platform` value.
 * @param {string} [options.filesystemType] Pre-probed filesystem type; when
 *   omitted the application root is probed.
 * @param {string} [options.homeDir] Home directory override, for tests.
 * @returns {{ targetDir: string, source: string, filesystemType: string }}
 *   `source` is `explicit`, `default`, or `relocated-posix`.
 */
export function resolveCargoBuildTargetDir({
  appRoot,
  appId,
  env = process.env,
  platform = process.platform,
  filesystemType,
  homeDir = homedir(),
} = {}) {
  if (!appRoot) {
    throw new Error('resolveCargoBuildTargetDir requires an appRoot');
  }
  if (!appId) {
    throw new Error('resolveCargoBuildTargetDir requires an appId');
  }

  const explicit = String(env.CARGO_TARGET_DIR ?? '').trim();
  if (explicit) {
    return {
      targetDir: path.resolve(appRoot, explicit),
      source: 'explicit',
      filesystemType: '',
    };
  }

  const defaultTargetDir = path.join(appRoot, 'target');
  if (platform !== 'linux') {
    return { targetDir: defaultTargetDir, source: 'default', filesystemType: '' };
  }

  const probed = filesystemType ?? probeFilesystemType(appRoot);
  if (!isWindowsPassthroughFilesystem(probed)) {
    return { targetDir: defaultTargetDir, source: 'default', filesystemType: probed };
  }

  return {
    targetDir: path.join(resolveCacheRoot({ env, homeDir }), ...RELOCATED_TARGET_NAMESPACE, appId),
    source: 'relocated-posix',
    filesystemType: probed,
  };
}

/**
 * One-line human description of the resolved target directory, for build logs.
 *
 * @param {{ targetDir: string, source: string, filesystemType: string }} resolved
 * @returns {string} Human-readable explanation.
 */
export function describeCargoBuildTargetDir(resolved) {
  const targetDir = resolved?.targetDir ?? '';
  switch (resolved?.source) {
    case 'explicit':
      return `cargo target dir: ${targetDir} (CARGO_TARGET_DIR)`;
    case 'relocated-posix':
      return `cargo target dir: ${targetDir} (relocated off the ${resolved.filesystemType || 'passthrough'} mount; set CARGO_TARGET_DIR to override)`;
    default:
      return `cargo target dir: ${targetDir}`;
  }
}

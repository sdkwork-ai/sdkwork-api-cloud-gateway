/**
 * Preflight probes for the Linux release builds.
 *
 * The gateway's `foundation-aiot` surface reaches `sdkwork-aiot-adapter-xiaozhi`
 * -> `audiopus` -> `audiopus_sys`. On Linux that crate links the host Opus
 * through `pkg-config` and, when `pkg-config` cannot find it, falls back to
 * building Opus from source with `autogen.sh`/`autoreconf`. A build guest with
 * neither the runtime library nor the autotools chain therefore dies *inside*
 * the dependency graph, after the compiler has already produced gigabytes of
 * artifacts — tens of minutes of work thrown away, with a message from
 * `audiopus_sys`'s build script rather than from the packaging entrypoint.
 *
 * These probes run before a build starts so the packaging entrypoint can fail
 * in milliseconds with the exact remediation command.
 */

import { spawnSync as defaultSpawnSync } from 'node:child_process';
import process from 'node:process';

/** `pkg-config` module name shipped by `libopus-dev` / `libopus-devel`. */
export const OPUS_PKG_CONFIG_MODULE = 'opus';

/** Toolchain the Opus autotools fallback shells out to. */
export const OPUS_AUTOTOOLS_COMMANDS = Object.freeze(['autoreconf', 'libtoolize']);

/** Remediation line for the Debian/Ubuntu container and WSL build guests. */
export const LINUX_BUILD_PREREQUISITE_HINT =
  'install the build prerequisites in the guest: '
  + 'sudo apt-get install -y pkg-config libopus-dev '
  + '(or the autotools fallback: autoconf automake libtool)';

/**
 * Whether `pkg-config` can resolve a module.
 *
 * @param {string} moduleName pkg-config module name.
 * @param {typeof defaultSpawnSync} [spawn] Injectable process runner.
 * @returns {boolean} True when the module is discoverable.
 */
export function hasPkgConfigModule(moduleName, spawn = defaultSpawnSync) {
  if (!moduleName) {
    return false;
  }
  const result = spawn('pkg-config', ['--exists', moduleName], {
    stdio: 'ignore',
    windowsHide: true,
  });
  return Boolean(result) && !result.error && result.status === 0;
}

/**
 * Whether every command is on `PATH`.
 *
 * @param {readonly string[]} commands Command names to look up.
 * @param {typeof defaultSpawnSync} [spawn] Injectable process runner.
 * @returns {boolean} True when all commands resolve.
 */
export function hasAllCommands(commands, spawn = defaultSpawnSync) {
  const missing = commands.filter((command) => {
    const result = spawn('sh', ['-c', `command -v ${command} >/dev/null 2>&1`], {
      stdio: 'ignore',
      windowsHide: true,
    });
    return !result || result.error || result.status !== 0;
  });
  return missing.length === 0;
}

/**
 * Required build prerequisites that are missing on this Linux guest.
 *
 * An explicit `LIBOPUS_LIB_DIR` short-circuits the probe: the crate links the
 * archive it points at and never enters the pkg-config or autotools path.
 *
 * @param {object} [options]
 * @param {Record<string, string | undefined>} [options.env] Environment to read.
 * @param {string} [options.platform] `process.platform` value.
 * @param {typeof defaultSpawnSync} [options.spawn] Injectable process runner.
 * @returns {string[]} Human-readable issues; empty when the guest can build.
 */
export function resolveLinuxBuildPrerequisiteIssues({
  env = process.env,
  platform = process.platform,
  spawn = defaultSpawnSync,
} = {}) {
  if (platform !== 'linux') {
    return [];
  }
  if (String(env.LIBOPUS_LIB_DIR ?? '').trim()) {
    return [];
  }
  if (hasPkgConfigModule(OPUS_PKG_CONFIG_MODULE, spawn)) {
    return [];
  }
  if (hasAllCommands(OPUS_AUTOTOOLS_COMMANDS, spawn)) {
    return [];
  }
  return [
    `Opus is unavailable to pkg-config (module '${OPUS_PKG_CONFIG_MODULE}') and the `
    + `${OPUS_AUTOTOOLS_COMMANDS.join('/')} fallback is missing; ${LINUX_BUILD_PREREQUISITE_HINT}`,
  ];
}

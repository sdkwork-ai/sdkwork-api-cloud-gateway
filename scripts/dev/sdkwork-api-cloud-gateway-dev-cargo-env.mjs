/**
 * Cargo environment for the gateway development build.
 *
 * The machine-global Cargo configuration on this workstation
 * (`~/.cargo/config.toml`) sets `[build] incremental = true`, and Rust enables
 * incremental compilation for the `dev` profile by default. Neither is
 * appropriate for this process: a gateway development build compiles the
 * embedded foundation assemblies, and incremental compilation keeps a
 * per-crate session directory plus a large query cache for every unit, which
 * is what makes the dev build both slower and prone to the incremental rustc
 * crash documented in `docs/architecture/tech/TECH_ARCHITECTURE.md`
 * ("Release builds on Windows may require `CARGO_INCREMENTAL=0` when rustc
 * hits a known `windows-sys` ICE in incremental release mode").
 *
 * `pnpm dev` therefore runs its cargo build with incremental compilation
 * disabled, which is also the posture the packaging path already uses
 * (`scripts/package-server.mjs`, `scripts/build-api-cloud-gateway-container.mjs`
 * both pin `CARGO_INCREMENTAL=0`). The development command is not exempt:
 *
 * - An explicit `CARGO_INCREMENTAL` in the caller's environment wins, so a
 *   developer can still opt back in for a single run.
 * - `SDKWORK_API_CLOUD_GATEWAY_ALLOW_CARGO_INCREMENTAL=1` opts the runner back
 *   in without exporting a Cargo variable.
 */

import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const allowIncrementalEnvironmentKey =
  'SDKWORK_API_CLOUD_GATEWAY_ALLOW_CARGO_INCREMENTAL';

/**
 * Resolve the Cargo environment for the development build.
 *
 * Returns only the Cargo variables this runner owns; the caller merges the
 * result over the profile environment so unrelated values stay untouched.
 *
 * @param {{ env?: Record<string, string | undefined>, platform?: string }} [options]
 * @returns {Record<string, string>} Cargo environment overrides.
 */
export function resolveDevelopmentCargoEnv({
  env = process.env,
  platform = process.platform,
} = {}) {
  const explicitIncremental = firstNonEmpty(env.CARGO_INCREMENTAL);
  if (explicitIncremental) {
    return {};
  }
  if (firstNonEmpty(env[allowIncrementalEnvironmentKey]) === '1') {
    return {};
  }
  if (platform !== 'win32') {
    return {};
  }
  return { CARGO_INCREMENTAL: '0' };
}

/**
 * Resolve the prebuilt Opus library that `audiopus_sys` must link on the
 * Windows GNU target.
 *
 * `platform-foundation-suite` embeds the AIoT surface, whose Xiaozhi adapter
 * depends on `audiopus`. On `x86_64-pc-windows-gnu` that crate's build script
 * takes a Unix-only source-build path (`cp`, `sh autogen.sh`, `make`) which
 * cannot run on Windows, and fails before compilation. Its documented escape
 * hatch is `OPUS_LIB_DIR`/`LIBOPUS_LIB_DIR`, which links a prebuilt library and
 * skips that path; `scripts/build-windows-libopus.ps1` produces exactly that
 * library from the source vendored inside the crate, using only the MinGW
 * toolchain already on the host.
 *
 * Returns an empty object when the caller already configured Opus, when the
 * platform is not Windows, or when the library has not been built yet — in the
 * last case the runner reports the remediation instead of failing the build
 * with an opaque linker error.
 *
 * @param {{
 *   env?: Record<string, string | undefined>,
 *   platform?: string,
 *   appRoot?: string,
 *   fileExists?: (filePath: string) => boolean,
 * }} [options]
 * @returns {{ env: Record<string, string>, libraryDirectory: string | undefined }}
 */
export function resolveDevelopmentOpusEnv({
  env = process.env,
  platform = process.platform,
  appRoot = defaultAppRoot(),
  fileExists = existsSync,
} = {}) {
  if (platform !== 'win32') {
    return { env: {}, libraryDirectory: undefined };
  }
  const configured = firstNonEmpty(env.LIBOPUS_LIB_DIR, env.OPUS_LIB_DIR);
  if (configured) {
    return { env: {}, libraryDirectory: configured };
  }
  const libraryDirectory = path.join(appRoot, 'target', 'prebuilt-opus', 'lib');
  if (!fileExists(path.join(libraryDirectory, 'libopus.a'))) {
    return { env: {}, libraryDirectory: undefined };
  }
  return {
    // OPUS_STATIC pins the static archive; the prebuilt path is what skips the
    // Unix-only source build inside audiopus_sys.
    env: { OPUS_LIB_DIR: libraryDirectory, OPUS_STATIC: '1' },
    libraryDirectory,
  };
}

function defaultAppRoot() {
  return path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
}

function firstNonEmpty(...values) {
  for (const value of values) {
    if (typeof value !== 'string') {
      continue;
    }
    const trimmed = value.trim();
    if (trimmed) {
      return trimmed;
    }
  }
  return undefined;
}

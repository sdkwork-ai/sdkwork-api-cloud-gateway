#!/usr/bin/env node
/**
 * Resolves the Rust target triple the current run compiles for.
 *
 * The release link reaches `audiopus_sys`, and that crate picks its Opus
 * strategy from the target environment, not the operating system:
 *
 * - `windows-msvc` links the `opus.lib` archive the crate ships under
 *   `msvc/x64`; nothing has to be built.
 * - everything Unix plus `windows-gnu` takes the vendored source path, which
 *   needs a system Opus or a prebuilt archive handed in through
 *   `LIBOPUS_LIB_DIR`.
 *
 * Treating "platform is Windows" as "needs a prebuilt MinGW archive" is what
 * broke the Windows release job: GitHub's `windows-2022` runner defaults to
 * `x86_64-pc-windows-msvc`, so the MinGW build was pure overhead and failed on
 * a registry path only the author's mirror-configured machine has.
 */
import { execFileSync } from 'node:child_process';
import process from 'node:process';

/** Returns the target triple, honouring an explicit `CARGO_BUILD_TARGET`. */
export function resolveRustTargetTriple() {
  const override = process.env.CARGO_BUILD_TARGET?.trim();
  if (override) return override;

  try {
    const version = execFileSync('rustc', ['-vV'], { encoding: 'utf8' });
    const hostLine = version
      .split('\n')
      .map((line) => line.trim())
      .find((line) => line.startsWith('host:'));
    if (hostLine) return hostLine.slice('host:'.length).trim();
  } catch {
    // rustc not on PATH: report unknown rather than guessing a target.
  }

  return '';
}

/**
 * True when the target takes `audiopus_sys`'s source path on Windows and so
 * needs `LIBOPUS_LIB_DIR` pointing at a prebuilt `libopus.a`.
 */
export function needsPrebuiltWindowsOpus() {
  return resolveRustTargetTriple().endsWith('windows-gnu');
}

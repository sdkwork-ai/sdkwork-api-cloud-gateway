#!/usr/bin/env node
/**
 * CI-layout rehearsal verification (DEPENDENCY_MANAGEMENT_SPEC.md §5.2).
 *
 * Runs inside the simulate-release-build.mjs throwaway checkout where the
 * repository and every sibling are side-by-side exactly like GitHub runners.
 * Verifies that the Cargo workspace dependency graph resolves with --locked
 * in that layout; a resolution failure here means the workflow package job
 * will fail the same way on GitHub.
 */
import { spawnSync } from 'node:child_process';

const args = [
  'metadata',
  '--format-version',
  '1',
  '--locked',
  '--no-default-features',
  '--features',
  'platform-foundation-suite',
];
console.log(`[ci-sim-verify] cargo ${args.join(' ')}`);
const result = spawnSync('cargo', args, {
  cwd: process.cwd(),
  stdio: 'inherit',
  maxBuffer: 64 * 1024 * 1024,
});
if (result.error) {
  console.error(`[ci-sim-verify] cargo failed to start: ${result.error.message}`);
  process.exit(1);
}
if (result.status !== 0) {
  console.error(`[ci-sim-verify] FAILED: cargo metadata exited ${result.status}`);
  process.exit(result.status ?? 1);
}
console.log('[ci-sim-verify] PASSED: Cargo workspace resolves in CI layout');

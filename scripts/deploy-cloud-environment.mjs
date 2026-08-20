#!/usr/bin/env node
/**
 * Cloud-profile apply is not a Docker Compose stack.
 *
 * Standalone WSL/Linux containers use `pnpm deploy:apply:standalone`.
 * Cloud artifacts are packaged with `pnpm gateway:package:cloud`.
 */
import process from 'node:process';

process.stderr.write(
  'deploy:apply:cloud does not start Docker Compose stacks. Use `pnpm deploy:apply:standalone -- <environment|all>` for WSL/Linux containers, or `pnpm gateway:package:cloud` for cloud artifacts.\n',
);
process.exit(2);

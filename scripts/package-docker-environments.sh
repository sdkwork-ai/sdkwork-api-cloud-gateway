#!/bin/bash
# Thin shim onto the canonical per-environment bundle packager
# (scripts/docker/package-environment-bundles.mjs).
#
# One implementation, two entry surfaces: the public build:container:environments
# script and this shell script both reach the same encapsulation. Environment
# lists, port plans, and validation live in the shared Node registry, never here.
#
# Usage: scripts/package-docker-environments.sh [--check] [--out <dir>] [--json]
# Output: dist/docker-packages/<environment>/
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root" && exec node scripts/docker/package-environment-bundles.mjs "$@"

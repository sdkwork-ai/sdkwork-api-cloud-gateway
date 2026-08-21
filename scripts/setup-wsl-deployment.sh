#!/bin/bash
# End-to-end WSL deployment entrypoint (thin compatibility shim).
#
# All real logic lives in the reusable toolkit scripts/deploy/apply.sh so there is a
# single source of truth and no duplicated deployment code. This shim keeps the
# historical `pnpm install:wsl` contract and the documented command name working.
#
# Usage:
#   scripts/setup-wsl-deployment.sh
#   scripts/setup-wsl-deployment.sh --skip-build
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

skip_build=""
for arg in "$@"; do
  case "$arg" in
    --skip-build) skip_build="--skip-build" ;;
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "unsupported option: $arg" >&2; exit 2 ;;
  esac
done

exec bash "$repo_root/scripts/deploy/apply.sh" $skip_build

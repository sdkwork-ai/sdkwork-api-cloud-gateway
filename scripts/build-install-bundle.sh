#!/usr/bin/env bash
# ============================================================================
# build-install-bundle.sh — one-command packaging pipeline (single bash entry).
#
# Wraps the full build chain so operators never touch node/pnpm directly:
#   1. scripts/package-server.mjs                    → cargo release build +
#      standalone server install tarball (dist/server/*.tar.gz)
#   2. scripts/build-api-cloud-gateway-container.mjs → OCI image
#      sdkwork-api-cloud-gateway:local
#   3. docker tag <local> → <repository>:<version>    → the image ref the
#      install bundle expects
#   4. scripts/docker/package-install-bundle.mjs      → self-contained bundle:
#      dist/docker-install/sdkwork-api-cloud-gateway-install-<version>.bundle/
#      (image.tar.gz + image.sha256 + deploy.sh + prepare-envs.sh + compose +
#      env templates + manifest)
#
# Usage:
#   bash scripts/build-install-bundle.sh [options]
#
# Options:
#   --skip-server-package  reuse the existing dist/server tarball (skip the
#                          cargo release build; use when Rust sources unchanged)
#   --skip-image-build     reuse the existing <repository>:<version> image
#                          (skip steps 2-3; use for env/compose-only changes)
#   --version <semver>     override version (default: release.currentVersion
#                          from sdkwork.app.config.json)
#   --dry-run              bundle step prints its plan only (build steps still
#                          run unless skipped)
#   -h, --help             show this help
#
# Build-machine prerequisites: Docker (with compose), Node.js (runs the
# packaging scripts), Rust toolchain (cargo) — the WSL Ubuntu 22.04 workflow
# is documented in docs/guides/operator/docker-install.md §2.
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "$REPO_ROOT"

LOG_PREFIX="[sdkwork-gateway-build]"
info() { printf '%s %s\n' "$LOG_PREFIX" "$*"; }
die()  { printf '%s ERROR: %s\n' "$LOG_PREFIX" "$*" >&2; exit 1; }
usage() { sed -n '2,/^set -euo pipefail$/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0; }

SKIP_SERVER_PACKAGE=0
SKIP_IMAGE_BUILD=0
DRY_RUN=0
VERSION="$(sed -n 's/.*"currentVersion"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' sdkwork.app.config.json | head -1)"
[[ -n "$VERSION" ]] || die "cannot read release.currentVersion from sdkwork.app.config.json"
IMAGE_REPOSITORY="registry.sdkwork.com/apps/sdkwork-api-cloud-gateway"
IMAGE_REF="${IMAGE_REPOSITORY}:${VERSION}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-server-package) SKIP_SERVER_PACKAGE=1 ;;
    --skip-image-build) SKIP_IMAGE_BUILD=1 ;;
    --version) VERSION="$2"; IMAGE_REF="${IMAGE_REPOSITORY}:${VERSION}"; shift ;;
    --dry-run) DRY_RUN=1 ;;
    -h|--help) usage ;;
    *) die "unknown option: $1 (see --help)" ;;
  esac
  shift
done

# Step 1: release build + server install tarball (cargo is incremental; only
# Rust sources changed since the last build are recompiled).
if [[ "$SKIP_SERVER_PACKAGE" -eq 1 ]]; then
  info "step 1/4 skipped (--skip-server-package): reusing dist/server tarball"
else
  info "step 1/4: cargo release build + server package (cold build ~10-18 min, incremental much faster)"
  node scripts/package-server.mjs package --deployment-profile standalone
fi
SERVER_TARBALL="$(ls -t dist/server/sdkwork-api-cloud-gateway-*-linux-x64-standalone-server.tar.gz 2>/dev/null | head -1 || true)"
[[ -n "$SERVER_TARBALL" ]] || die "server tarball missing under dist/server/ — run without --skip-server-package"
info "server package: ${SERVER_TARBALL}"

# Step 2: OCI container image.
if [[ "$SKIP_IMAGE_BUILD" -eq 1 ]]; then
  info "step 2/4 skipped (--skip-image-build)"
else
  info "step 2/4: build container image sdkwork-api-cloud-gateway:local"
  node scripts/build-api-cloud-gateway-container.mjs
fi

# Step 3: align the image ref the install bundle expects.
if docker image inspect "$IMAGE_REF" > /dev/null 2>&1; then
  info "step 3/4: image ${IMAGE_REF} already present"
elif docker image inspect "sdkwork-api-cloud-gateway:local" > /dev/null 2>&1; then
  info "step 3/4: tagging sdkwork-api-cloud-gateway:local -> ${IMAGE_REF}"
  docker tag sdkwork-api-cloud-gateway:local "$IMAGE_REF"
else
  die "image sdkwork-api-cloud-gateway:local missing — run without --skip-image-build"
fi

# Step 4: self-contained install bundle.
info "step 4/4: package install bundle (version ${VERSION})"
BUNDLE_ARGS=(--version "$VERSION" --skip-image-build)
if [[ "$DRY_RUN" -eq 1 ]]; then BUNDLE_ARGS+=(--dry-run); fi
node scripts/docker/package-install-bundle.mjs "${BUNDLE_ARGS[@]}"

BUNDLE_DIR="dist/docker-install/sdkwork-api-cloud-gateway-install-${VERSION}.bundle"
info "done. bundle: ${BUNDLE_DIR}"
info "next: copy the bundle to the target host and run:"
info "  bash prepare-envs.sh && bash deploy.sh --environment <env> [--embedded] [--replicas N]"

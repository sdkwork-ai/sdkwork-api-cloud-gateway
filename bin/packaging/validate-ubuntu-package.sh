#!/usr/bin/env bash

set -euo pipefail

archive=${1:-dist/server/sdkwork-api-cloud-gateway-0.1.0-linux-x64-standalone-server.tar.gz}
environment=${2:-production}
deployment_profile=${3:-standalone}

case "$environment" in
  development|test|staging|production) ;;
  *) echo "unsupported environment: $environment" >&2; exit 2 ;;
esac
case "$deployment_profile" in
  standalone|cloud) ;;
  *) echo "unsupported deployment profile: $deployment_profile" >&2; exit 2 ;;
esac

if [[ ! -f "$archive" || ! -f "$archive.sha256" ]]; then
  echo "archive and checksum sidecar are required: $archive" >&2
  exit 2
fi

stage_name=$(basename "$archive" .tar.gz)
validation_root=$(mktemp -d /tmp/sdkwork-gateway-install.XXXXXX)
case "$validation_root" in
  /tmp/sdkwork-gateway-install.*) ;;
  *) echo "unsafe validation root: $validation_root" >&2; exit 90 ;;
esac
trap 'rm -rf -- "$validation_root"' EXIT

archive_dir=$(dirname "$archive")
archive_name=$(basename "$archive")
(
  cd "$archive_dir"
  sha256sum --quiet -c "$archive_name.sha256" # PORTABILITY:target-linux
)
tar -xzf "$archive" -C "$validation_root"
package_root="$validation_root/$stage_name"
if [[ ! -d "$package_root" ]]; then
  echo "archive does not contain canonical stage root: $stage_name" >&2
  exit 1
fi

(
  cd "$package_root"
  sha256sum --quiet -c checksums.sha256 # PORTABILITY:target-linux
  bash install/install-ubuntu.sh \
    --environment "$environment" \
    --deployment-profile "$deployment_profile" \
    --destdir "$validation_root/stage" \
    --no-start
)

unit_root="$validation_root/stage/usr/lib/systemd/system"
install_root="$validation_root/stage/opt/sdkwork/api-gateway"
test -f "$unit_root/sdkwork-api-gateway.service"
test -f "$unit_root/sdkwork-knowledgebase-rpc.service"
"$install_root/bin/sdkwork-api-cloud-gateway" --version
"$install_root/bin/sdkwork-intelligence-knowledgebase-rpc-bin" --version

printf 'Ubuntu staged install passed: %s (%s.%s)\n' "$archive" "$deployment_profile" "$environment"

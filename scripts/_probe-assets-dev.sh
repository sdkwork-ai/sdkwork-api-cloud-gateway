#!/bin/bash
set -euo pipefail
echo "==> doubled path without token"
curl -sS -o /tmp/double-body.json -w "status=%{http_code}\n" \
  http://127.0.0.1:3910/app/v3/api/app/v3/api/assets \
  -H "Host: api-dev.sdkwork.com"
python3 - <<'PY'
import json
print(json.dumps(json.load(open("/tmp/double-body.json")), indent=2, ensure_ascii=False))
PY
echo "==> recent fallback/access for doubled path"
docker logs sdkwork-gateway-development-gateway-1 2>&1 | grep -E "app/v3/api/app/v3/api/assets|path=.*/assets" | tail -15
echo "==> git log web-axum normalize"
git -C /mnt/e/sdkwork-space/sdkwork-web-framework log -5 --oneline -- crates/sdkwork-web-axum/src/middleware.rs

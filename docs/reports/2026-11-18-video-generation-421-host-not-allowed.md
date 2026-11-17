# 2026-11-18 — Video generation `421 host_not_allowed`: the gateway rejecting its own registry endpoint

## 1. Incident

Video generation failed with the platform envelope below:

```json
{
  "code": 50201,
  "message": "provider error: http status 421: {\"type\":\"about:blank\",\"title\":\"host_not_allowed\",\"detail\":\"request host is not allowed by gateway host allowlist\",\"status\":421,\"code\":40001,\"traceId\":\"46d72c9c-...\",\"instance\":\"POST /vidu/ent/v2/text2video\"}",
  "traceId": "a4c4d517-..."
}
```

Host: `ubuntu@api.noaper.com`; container
`sdkwork-api-cloud-gateway-production-i1-gateway-1`, image `0.1.85`.

## 2. Evidence (read-only, no deployment)

- Gateway audit log, tracing target `sdkwork_api_cloud_gateway::audit`:

  ```text
  category: "security", action: "reject", reason: "host_not_allowed",
  status: 421, method: "POST", route_target: "dependency",
  service_id: Some("sdkwork-cloudrouter-open-api")
  ```

  - 1,725 rejections between container start and the investigation;
  - every `POST` rejection with `route_target: "dependency"` /
    `service_id: "sdkwork-cloudrouter-open-api"` is a `/vidu/ent/...` hop;
  - each one is nested inside a gateway request span for
    `POST /app/v3/api/generations/videos/text_to_video`, i.e. the generations
    route ran in this process and its provider call to `cloudrouter` was
    rejected before routing;
  - the remaining `GET`/`POST` `route_target: "unmatched"` rejections are the
    same defect (background registry consumers dialling `/healthz` and other
    peer paths).

- Discovery registry (`discovery_service_instance`, live rows) advertises this
  process as an IP literal on the container bridge:

  ```text
  cloudrouter  standalone.production.i1:7d5d32b7c2a6:8080  http://172.18.0.5:8080  serving
  generations  standalone.production.i1:7d5d32b7c2a6:8080  http://172.18.0.5:8080  serving
  ```

  `SDKWORK_API_CLOUD_GATEWAY_PRIVATE_HTTP_URL` is empty, so the endpoint is the
  zero-config `private-preferred interface` decision.

- Live reproduction from inside the container:

  ```text
  ip-host:421      # curl http://172.18.0.5:8080/readyz
  alias-host:200   # curl -H 'Host: gateway' http://172.18.0.5:8080/readyz
  ```

- Container env: `SDKWORK_API_CLOUD_GATEWAY_ALLOWED_HOSTS` enumerates the public
  domain families plus `gateway` / `sdkwork-api-cloud-gateway`, and
  `SDKWORK_API_CLOUD_GATEWAY_SECURITY_ALLOW_IP_HOSTS=false` (production
  posture). No `nginx` access log on the host contains `vidu`: this traffic is
  east-west inside the process, not public edge traffic.

## 3. Root cause

The gateway is a composition root. `sdkwork-generations` (video/image media
providers) resolves `cloudrouter` through the database-backed HTTP service
registry and calls the advertised endpoint per SDK call. Because the registry
advertises the private bridge address (`http://172.18.0.5:8080`), the request
comes back into the same process with an IP-literal `Host`.

`security::validate_allowed_host` only compared that `Host` against
`[security].allowedHosts` (registered domains plus aliases) and the explicit
`allowIpHosts` switch, both of which are off for production. The gateway
therefore rejected its own east-west hop with `421 host_not_allowed`, and the
generations adapter wrapped it as `50201 provider error`.

This is a regression of the registry migration (build 0.1.37+): before it, the
in-process media/chat hop used a loopback default (`127.0.0.1:3900`) which the
allowlist already contained.

## 4. Fix (source, 0.1.85+)

- `sdkwork-api-cloud-gateway-config::GatewaySecurityConfig` gains
  `own_hosts` (`#[serde(skip)]`), the normalized hosts of the socket this
  process publishes. It is never loaded from TOML or the environment.
- `security::validate_allowed_host` matches `allowed_hosts` **and** `own_hosts`.
- `security::record_own_hosts` records the advertised registry origin, the
  concrete bind address, and `127.0.0.1` / `[::1]`.
- `service_registry::accept_own_published_origin` is called by
  `listener_main` right after the listener binds and **before** the registry
  starts and the router is assembled, so every SDK client built during
  assembly resolves to a `Host` the process accepts. Startup logs
  `gateway accepts its own published origin as a Host value` with the origins.
- Scope: only hosts that name this listener are admitted. A foreign IP literal
  or unknown domain still returns `421`; `allowedHosts` keeps deciding every
  other caller, and `GATEWAY_SECURITY_ALLOW_IP_HOSTS` is not needed (and stays
  false in production).

## 5. Verification

- `cargo test -p sdkwork-api-cloud-gateway --lib security::tests`
  — own-origin acceptance, foreign IP-literal rejection, origin normalization.
- `cargo test -p sdkwork-api-cloud-gateway-config`
  — config defaults unchanged (`own_hosts` is not part of the authored config).
- `cargo fmt -- --check`.
- Operator runbook updated: `docs/guides/operator/HTTP_SERVICE_REGISTRY.md`
  §2, §5, §6, §7 and `README.md` security section.

## 6. Rollout

Not performed here by instruction. Next release build must bump
`release.currentVersion` and deploy through `bin/docker-image.sh` /
`bin/docker-deploy.sh`; the fix needs no bundle env change. After the deploy,
confirm `GET http://172.18.0.5:8080/readyz` from inside the gateway container
returns `200` and that the audit stream no longer records
`reason: "host_not_allowed"` for east-west calls.

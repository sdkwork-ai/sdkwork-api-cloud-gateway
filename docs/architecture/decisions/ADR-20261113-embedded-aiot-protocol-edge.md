# ADR-20261113 Embedded AIoT Protocol Edge

Status: accepted
Requirement: REQ-2026-1113-platform-gateway-aiot-edge-hosting
Owner: SDKWork platform gateway repository
Date: 2026-11-13
Specs: `APP_RUNTIME_TOPOLOGY_SPEC.md`, `APPLICATION_GATEWAY_SPEC.md`, `specs/component.spec.json`

## Context

The platform cloud gateway is an Axum HTTP host. AIoT device protocols
(HTTP/WebSocket on a TCP ingress, the Xiaozhi UDP media bridge, and the MQTT
bridge) required the separate `sdkwork-aiot-device-edge-runtime` process, even
when the AIoT HTTP APIs were already embedded in the gateway. Device-facing
ingress therefore needed another deployed process and a second lifecycle.

The AIoT edge runtime is now embeddable: its listener/worker loop lives in the
library (`spawn_device_edge_runtime`) and returns a shutdown handle.

## Decision

- Add the `foundation-aiot-realtime` Cargo feature (requires
  `foundation-aiot` and `foundation-appbase`; included in
  `platform-foundation-suite-container`).
- The gateway starts `sdkwork_aiot_device_edge_runtime::spawn_device_edge_runtime`
  in `embedded_dependency_routes.rs` after the AIoT HTTP assembly bootstraps,
  and retains the returned handle.
- Runtime configuration lives in `[aiotRealtime]`:
  `enabled`, optional `bind`, and `mqttBridgeEnabled`, with
  `SDKWORK_API_CLOUD_GATEWAY_AIOT_REALTIME_*` overrides. Protocol tuning stays
  in the existing `SDKWORK_AIOT_*` environment family.
- The AIoT listener binds in addition to the gateway HTTP listener; startup
  fails if the AIoT ingress cannot bind within the 10s readiness budget.
- `GatewayShutdown` stops the AIoT listeners after the HTTP listener drains.
- The MQTT bridge is a client of an external broker; no MQTT broker is embedded.

## Alternatives

- Keep AIoT ingress in the standalone edge process: rejected because it keeps
  a second deployable and prevents a single gateway process from starting the
  AIoT server capability.
- Mount the AIoT HTTP/WS upgrade on the Axum listener: rejected because the
  protocol edge owns raw WebSocket framing, UDP media, and MQTT session state;
  a separate bind preserves that boundary while staying in-process.

## Consequences

- One gateway process can own HTTP APIs, the AIoT TCP/WebSocket ingress, the
  UDP media bridge, and the MQTT bridge.
- Deployments enabling the feature must publish the AIoT ingress port
  (default `18080`), supply the `SDKWORK_AIOT_*` credential/pepper/intelligence
  environment in production, and configure an external MQTT broker when
  `mqttBridgeEnabled` is on.
- Production safety validation from the edge runtime still applies.

## Verification

- `cargo check -p sdkwork-api-cloud-gateway --no-default-features --features foundation-aiot-realtime`
- `cargo test -p sdkwork-api-cloud-gateway --no-default-features --features foundation-aiot-realtime --lib`
- `cargo test -p sdkwork-api-cloud-gateway-config`
- Start the gateway with `[aiotRealtime] enabled = true`, then check
  `/healthz`, `/iot/xiaozhi/ota`, the Xiaozhi WebSocket hello, and
  `/internal/bridge/health`.

## Supersedes / Superseded By

Nothing.

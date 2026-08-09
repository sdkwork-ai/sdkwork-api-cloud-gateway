# ADR-20260809 Embedded IM Realtime Plane

Status: accepted
Requirement: REQ-2026-0809-platform-gateway-realtime-hosting
Owner: SDKWork platform gateway repository
Date: 2026-08-09
Specs: `APP_RUNTIME_TOPOLOGY_SPEC.md`, `APPLICATION_GATEWAY_SPEC.md`, `specs/component.spec.json`, `specs/process-database-pool.spec.json`

> Note: this repository-level decision implements the global standards
> change `ADR-20260809-platform-gateway-realtime-hosting`, whose Status is
> `proposed` pending human review in the `sdkwork-specs` workspace.

## Context

The platform cloud gateway (`sdkwork-api-cloud-gateway`) is an HTTP-only host.
IM realtime connections (WebSocket upgrade plus TCP/UDP/QUIC client links)
required a separate IM process even in `cloud` deployments. The global
standards change in
`ADR-20260809-platform-gateway-realtime-hosting`
(`../sdkwork-specs/docs/architecture/decisions/ADR-20260809-platform-gateway-realtime-hosting.md`)
permits the deployed platform cloud gateway to host a declared application
realtime plane.

## Decision

- Add the opt-in Cargo feature `foundation-im-realtime` (requires
  `foundation-im`; not part of `platform-foundation-suite`) that embeds
  `session-gateway` from the `sdkwork-im` workspace.
- The gateway bootstraps the embedded realtime plane
  (`session_gateway::bootstrap_gateway_embedded_realtime_plane`) before the IM
  API assembly and wires the assembly with
  `assemble_api_router_with_realtime_bootstrap`, so the WebSocket upgrade at
  `/im/v3/api/realtime/ws` and the declared TCP/UDP/QUIC link listeners run in
  the gateway process.
- Realtime hosting requires `realtime.enabled = true` (runtime TOML) or
  `SDKWORK_API_CLOUD_GATEWAY_REALTIME_ENABLED=true`, plus the IM realtime
  environment family (`SDKWORK_IM_REALTIME_*`). The IM standalone gateway
  keeps its existing single-process mode for `standalone` profiles.
- After the HTTP listener's graceful shutdown, the gateway drains the plane
  (route fence, link listeners, cluster subscriber) with
  `SDKWORK_IM_SESSION_GATEWAY_DRAIN_TIMEOUT_SECS` (default 45s).
- The IM realtime PostgreSQL pool (`PostgresRealtimePool`, default max 32) and
  its Redis clients are registered as an IM-owned process-pool exception in
  `specs/process-database-pool.spec.json`.

## Alternatives

- Keep realtime termination on the IM process in `cloud` profiles: rejected in
  the global ADR because it forces an extra deployed process per realtime
  application.
- Extend `foundation-im` itself: rejected because it would change existing
  HTTP-only IM embedding behavior and force realtime runtime dependencies on
  all `foundation-im` consumers.

## Consequences

- The gateway process can terminate IM HTTP APIs and realtime connections in
  one process when the feature is enabled; deployments opt in.
- The gateway must supply the IM realtime environment (database URL, optional
  Redis cluster bus, node identity, transport binds, QUIC TLS material) and
  honor the plane's production fail-closed rules.
- The gateway never installs the IM tracing subscriber; it keeps its own.

## Verification

- `cargo check -p sdkwork-api-cloud-gateway --features foundation-im-realtime`
- `cargo test -p sdkwork-api-cloud-gateway --features foundation-im-realtime`
- `tests/realtime_embed_tests.rs` (WebSocket handshake through the gateway
  listener, TCP link transport bind, plane drain)
- `pnpm check`, `pnpm verify`, `pnpm gateway:matrix:standalone`

## Supersedes / Superseded By

Nothing; implements `ADR-20260809-platform-gateway-realtime-hosting` in the
`sdkwork-specs` workspace.

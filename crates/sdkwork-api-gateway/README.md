# sdkwork-api-gateway

Domain: platform
Capability: api-gateway-runtime
Package type: Rust gateway library crate
Status: standard

## Public API

This crate owns Axum router construction, gateway runtime assembly, preflight validation, dependency
proxying, embedded router delegation, request policy enforcement, traffic controls, security
controls, diagnostics, metrics, and resilience behavior.

Public router builders include `build_sdkwork_api_gateway_api_server_router`,
`build_sdkwork_api_gateway_router`, `build_sdkwork_api_gateway_router_with_registry`, and
`build_sdkwork_api_gateway_router_with_embedded_routers`. When the `foundation-appbase` feature is
enabled, the crate also exports Appbase embedded router helpers and
`SdkworkAppbaseLocalIamRuntimeContext`.

## Required SDK Surface

This crate does not regenerate dependency SDKs and does not call dependency APIs through handwritten
business clients. Split-mode HTTP/WebSocket traffic is proxied to configured upstreams; embedded
mode delegates to executable routers supplied through public integration entrypoints.

## Configuration

The runtime consumes `GatewayRuntimeConfig` from `sdkwork-api-gateway-config`, route metadata from
`sdkwork-api-gateway-registry`, and observability projections from
`sdkwork-api-gateway-observability`. Embedded foundation integrations are activated through Cargo
features and workspace dependencies, not through a standalone gateway catalog.

## SaaS/Private/Local Behavior

The same router builders support independent server startup and same-process embedding by other Rust
applications. Split mode requires dependency upstream base URLs; embedded mode requires executable
routers for dependency surfaces that do not use upstreams.

## Security

Appbase IAM handlers remain dependency-owned. The runtime enforces configured host allowlists,
header limits, WAF rules, body limits, CORS, policy governance, request timeouts, rate limits,
concurrency limits, diagnostics safety, and bounded observability labels before or during proxying.

## Extension Points

Add a dependency API surface by updating the application manifest/component spec and, only when a
production-compatible router builder exists, adding an optional Cargo dependency and runtime feature.
Do not preserve retired router aliases or dependency-owned handlers in this crate.

## Verification

Run from the repository root:

```powershell
cargo test -p sdkwork-api-gateway
cargo test --workspace
```

Related specs: `../../../sdkwork-specs/COMPONENT_SPEC.md`,
`../../../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md`, `../../../sdkwork-specs/RUST_CODE_SPEC.md`,
`../../../sdkwork-specs/SECURITY_SPEC.md`, and `../../../sdkwork-specs/TEST_SPEC.md`.

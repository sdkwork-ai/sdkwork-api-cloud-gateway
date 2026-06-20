# sdkwork-api-gateway

Domain: platform
Capability: api-gateway-runtime
Package type: Rust gateway library crate
Owner: `sdkwork-api-gateway` maintainers
Status: standard

## Public API

This crate owns Axum router construction, gateway runtime assembly, preflight validation, dependency
proxying, embedded router delegation, request policy enforcement, traffic controls, security
controls, diagnostics, metrics, and resilience behavior.

Public router builders include `build_sdkwork_api_gateway_api_server_router`,
`build_sdkwork_api_gateway_router`, `build_sdkwork_api_gateway_router_with_registry`, and
`build_sdkwork_api_gateway_router_with_embedded_routers`. Split mode finalizes routers through
`sdkwork-web-framework` (`finalize_gateway_router`, `finalize_gateway_router_async`,
`split_proxy_call_chain`, `web_framework_enabled`, `parse_web_framework_toggle`). Toggle the outer
pipeline with `SDKWORK_API_GATEWAY_WEB_FRAMEWORK_ENABLED` (`true` by default); when disabled, split
mode falls back to Tower `CorsLayer` and gateway security middleware while still assigning UUID
`x-request-id`. When the `foundation-appbase` feature is enabled, the crate also exports Appbase
embedded router helpers. IAM tenant and organization context is resolved from dual tokens at request time, not injected through `SDKWORK_IAM_LOCAL_*` environment variables.

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

Appbase IAM handlers remain dependency-owned. Split mode runs `sdkwork-web-framework` surface
classification, request identity, logging, audit, baseline secure headers, and response identity
before proxy dispatch; gateway-specific WAF, host allowlist, header limits, body limits, CORS overlays,
policy governance, request timeouts, rate limits, concurrency limits, diagnostics safety, gateway
extended secure headers, and bounded observability labels remain gateway-owned controls during
proxying.

## Extension Points

Add a dependency API surface by updating the application manifest/component spec and, only when a
production-compatible router builder exists, adding an optional Cargo dependency and runtime feature.
Do not preserve retired router aliases or dependency-owned handlers in this crate.

## Verification

Run from the repository root:

```powershell
cargo test -p sdkwork-api-gateway
cargo test -p sdkwork-api-gateway --test web_framework_tests
cargo test --workspace
```

Related specs: `../../../sdkwork-specs/COMPONENT_SPEC.md`,
`../../../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md`, `../../../sdkwork-specs/RUST_CODE_SPEC.md`,
`../../../sdkwork-specs/WEB_FRAMEWORK_SPEC.md`, `../../../sdkwork-specs/SECURITY_SPEC.md`, and
`../../../sdkwork-specs/TEST_SPEC.md`.

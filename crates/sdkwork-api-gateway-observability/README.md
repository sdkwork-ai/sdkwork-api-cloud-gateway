# sdkwork-api-gateway-observability

Domain: platform
Capability: api-gateway-observability
Package type: Rust library crate
Owner: `sdkwork-api-gateway` maintainers
Status: standard

## Public API

This crate owns typed observability projections for SDKWork API Gateway. Public exports include
startup summaries, access log event builders, audit log event builders, and their input/event DTOs.

## Required SDK Surface

This crate does not consume generated SDK clients. It reads gateway config and route registry data
to build bounded, redacted observability records for runtime logging and diagnostics.

## Configuration

Observability behavior is driven by `GatewayRuntimeConfig`, especially the access log, audit log,
metrics, tracing, diagnostics, and redaction fields documented in `../../README.md` and configured
through `../../configs/` templates.

## SaaS/Private/Local Behavior

The same event shapes are used in SaaS, private, and local modes. Deployment-specific log sinks are
configured outside this crate by the API server process or hosting environment.

## Security

Access log builders redact configured sensitive headers. Event fields must remain bounded and must
not include raw tokens, tenant/user identifiers, IP addresses, trace secrets, upstream base URLs, or
unbounded request data.

## Extension Points

Add new event fields only when they are bounded, documented in `../../specs/component.spec.json`,
and covered by tests. Runtime metrics counters and histograms live in the gateway library crate.

## Verification

Run from the repository root:

```powershell
cargo test -p sdkwork-api-gateway-observability
```

Related specs: `../../../sdkwork-specs/OBSERVABILITY_SPEC.md`,
`../../../sdkwork-specs/SECURITY_SPEC.md`, `../../../sdkwork-specs/RUST_CODE_SPEC.md`, and
`../../../sdkwork-specs/TEST_SPEC.md`.

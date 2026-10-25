# sdkwork-api-cloud-gateway-registry

Domain: platform
Capability: api-gateway-route-registry
Package type: Rust library crate
Owner: `sdkwork-api-cloud-gateway` maintainers
Status: standard

## Public API

This crate owns deterministic route registry data for SDKWork API Gateway. Public exports include
`GatewayRouteRegistry`, `RouteDescriptor`, route protocol/visibility enums, execution target enums,
and route resolution helpers.

## Required SDK Surface

This crate does not call generated SDK clients. It consumes `DependencyApiSurfaceConfig` from
`sdkwork-api-cloud-gateway-config` and projects dependency API prefixes into executable gateway route
metadata.

## Configuration

Route registry instances are built from typed dependency surfaces. Fixed Appbase IAM routes and
surface-prefix dependency routes are derived from configuration rather than from a standalone gateway
catalog.

## SaaS/Private/Local Behavior

The same registry ordering is used for local, private, and SaaS deployments. More specific
dependency prefixes resolve before broad fallback prefixes so shared gateway composition remains
stable across deployment modes.

## Security

Appbase IAM route ownership stays dependency-owned. This crate must not add application-local IAM
handlers or infer authorization from route metadata; authorization stays with the dependency service
or verified runtime context.

## Extension Points

Add route descriptor fields only when they are needed by runtime routing, diagnostics, or policy
enforcement. New dependency surfaces should be declared in `../../agent.app.config.json` and then
projected through this registry.

## Verification

Run from the repository root:

```powershell
cargo test -p sdkwork-api-cloud-gateway-registry
```

Related specs: `../../../sdkwork-specs/API_SPEC.md`, `../../../sdkwork-specs/COMPONENT_SPEC.md`,
`../../../sdkwork-specs/RUST_CODE_SPEC.md`, and `../../../sdkwork-specs/TEST_SPEC.md`.

# sdkwork-api-gateway-config

Domain: platform
Capability: api-gateway-config
Package type: Rust library crate
Status: standard

## Public API

This crate owns typed runtime configuration for SDKWork API Gateway. Public exports include
`GatewayRuntimeConfig`, dependency surface and upstream config structs, gateway mode enums,
validation errors, default service identifiers, default upstream URLs, and supported environment key
constants.

## Required SDK Surface

This crate does not consume generated SDK clients. It describes dependency SDK surfaces from
`sdkwork.app.config.json` and safe TOML templates so the gateway runtime can proxy or embed those
surfaces through the standard gateway boundary.

## Configuration

Configuration inputs are application manifests, safe TOML templates under `../../configs/`, and
environment overrides declared in `../../sdkwork.app.config.json`. Local secret-bearing overrides
must stay outside source control.

## SaaS/Private/Local Behavior

The same config model supports SaaS, private, and local startup. Split mode requires dependency base
URLs, while embedded mode requires executable routers for dependency surfaces that run in-process.

## Security

Config validation fails closed for empty upstream identifiers, empty dependency prefixes, invalid
sample rates, zero body limits, and invalid policy or traffic rule identities. This crate must not
store live tokens, secrets, or tenant/user runtime state.

## Extension Points

Add new gateway config fields here when they are runtime-wide and validated before serving traffic.
Add new foundation dependency surfaces through `sdkwork.app.config.json` and keep Cargo integration
evidence in the gateway library crate when an embedded router exists.

## Verification

Run from the repository root:

```powershell
cargo test -p sdkwork-api-gateway-config
```

Related specs: `../../../sdkwork-specs/CONFIG_SPEC.md`, `../../../sdkwork-specs/ENVIRONMENT_SPEC.md`,
`../../../sdkwork-specs/RUST_CODE_SPEC.md`, and `../../../sdkwork-specs/TEST_SPEC.md`.

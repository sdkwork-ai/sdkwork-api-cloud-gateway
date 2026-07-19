# sdkwork-api-cloud-gateway-config

Domain: platform
Capability: api-gateway-config
Package type: Rust library crate
Owner: `sdkwork-api-cloud-gateway` maintainers
Status: standard

## Public API

This crate owns typed runtime configuration for SDKWork API Gateway. Public exports include
`GatewayRuntimeConfig`, embedded dependency surface config structs, gateway mode enums, validation
errors, default service identifiers, and supported environment key constants.

## Required SDK Surface

This crate does not consume generated SDK clients. It projects dependency SDK surfaces from
`../../specs/component.spec.json` and merges them with safe runtime TOML so the gateway can mount
standard assembly routers without duplicating a catalog in the app manifest or profile config.

## Configuration

Configuration inputs are application identity from `../../sdkwork.app.config.json`, dependency
surfaces from `../../specs/component.spec.json`, safe runtime TOML under `../../etc/`, and process
environment overrides. Local secret-bearing overrides must stay outside source control.

## SaaS/Private/Local Behavior

The same embedded config model supports SaaS, private, and local startup. Every dependency surface
requires a Cargo feature, assembly dependency, executable router export, and coverage evidence.

## Security

Config validation fails closed for unknown dependency fields, per-dependency base URLs, incomplete
assembly evidence, empty dependency prefixes, invalid sample rates, zero body limits, and invalid
policy or traffic rule identities. This crate must not store live tokens, secrets, or tenant/user
runtime state.

## Extension Points

Add new gateway config fields here when they are runtime-wide and validated before serving traffic.
Add new foundation dependency surfaces through `../../specs/component.spec.json` and keep matching
Cargo integration evidence in the gateway library crate when an embedded router exists.

## Verification

Run from the repository root:

```powershell
cargo test -p sdkwork-api-cloud-gateway-config
```

Related specs: `../../../sdkwork-specs/SOURCE_CONFIG_SPEC.md`, `../../../sdkwork-specs/CONFIG_SPEC.md`, `../../../sdkwork-specs/ENVIRONMENT_SPEC.md`,
`../../../sdkwork-specs/RUST_CODE_SPEC.md`, and `../../../sdkwork-specs/TEST_SPEC.md`.

# sdkwork-api-gateway-api-server

Domain: platform
Capability: api-gateway-api-server
Package type: Rust API server process crate
Owner: `sdkwork-api-gateway` maintainers
Status: standard

## Public API

This crate owns the independent SDKWork API Gateway server process. It exposes the
`sdkwork-api-gateway` binary and library helpers `load_gateway_config` and `config_path_from_args`
for callers that need the same manifest/config/env resolution as the binary.

## Required SDK Surface

This crate does not consume generated SDK clients. It starts the gateway runtime from the public
router builder exported by `sdkwork-api-gateway` and relies on dependency upstreams or embedded
routers configured by the gateway runtime.

## Configuration

Configuration is resolved from `--config`, `SDKWORK_API_GATEWAY_CONFIG`, the application manifest,
and environment overrides. Safe examples live under `../../configs/`; host-local secrets and live
deployment overrides must not be committed.

## SaaS/Private/Local Behavior

The binary is the standard server startup path for local development, private deployments, and CI
packaging. `pnpm dev` runs this crate with the integrated single-port development template, and
`pnpm package:server` packages the release binary with safe config templates.

## Security

This crate initializes tracing, loads config, binds the configured listener, and delegates request
handling to the gateway runtime. It must not implement dependency API handlers, IAM logic, or
business rules directly.

## Extension Points

Add process-level startup concerns here, such as config path resolution, tracing initialization, or
shutdown handling. Runtime routing, proxying, policy, and dependency integration belong in
`sdkwork-api-gateway`.

## Verification

Run from the repository root:

```powershell
cargo test -p sdkwork-api-gateway-api-server
pnpm dev
pnpm package:server
pnpm package:server:validate
```

Related specs: `../../../sdkwork-specs/RUST_CODE_SPEC.md`,
`../../../sdkwork-specs/CONFIG_SPEC.md`, `../../../sdkwork-specs/DEPLOYMENT_SPEC.md`, and
`../../../sdkwork-specs/TEST_SPEC.md`.

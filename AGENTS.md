# SDKWork API Gateway Agent Guide

This repository is an SDKWork application root. Before changing runtime behavior, config, API
surfaces, SDK wiring, or release metadata, follow the workspace rules in `../AGENTS.md` and the
canonical specs under `../sdkwork-specs/`.

## Application Identity

- Application id: `sdkwork-api-gateway`
- Runtime family: Rust backend service
- Primary service: `crates/sdkwork-api-gateway-api-server`
- Public crate entrypoint: `crates/sdkwork-api-gateway`

## Required Local Files

- `sdkwork.app.config.json` is the application manifest source of truth.
- `specs/component.spec.json` describes executable modules and API dependency surfaces.
- `configs/*.toml.example` files are tracked templates only. Do not place live tokens or secrets in
  tracked files.

## Build And Test

Run commands from this repository root:

- `cargo fmt --all -- --check`
- `cargo test --workspace`

## Runtime Rules

- The gateway supports independent startup through `crates/sdkwork-api-gateway-api-server`.
- Third-party applications embed the gateway through public router builders from
  `sdkwork-api-gateway`.
- Appbase IAM routes are dependency routes. Do not reimplement `/app/v3/api/auth`,
  `/app/v3/api/iam`, `/app/v3/api/oauth`, or `/app/v3/api/system/iam` handlers in this product.
- Split mode requires upstream dependency base URLs at startup.
- Embedded mode requires an executable embedded router for each required dependency surface when no
  upstream is configured.
